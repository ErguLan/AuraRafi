#![allow(non_upper_case_globals)] // Preserve the operating-system SDK identifiers in match arms.
use std::sync::{Arc, atomic::Ordering};
use std::hash::{Hash, Hasher};
use windows::core::{implement, Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, VARIANT, BSTR, PCWSTR};
use windows::Win32::{Foundation::{BOOL, HWND}, UI::Accessibility::*, System::{Com::SAFEARRAY, Ole::{SafeArrayCreateVector, SafeArrayPutElement, SafeArrayDestroy}, Variant::VT_I4, Threading::GetCurrentThreadId}};
use raf_ui::{UiAccessibilityRole as Role, automation::UiAutomationAction as Action};
use super::{Shared, snapshot::{Element, Snapshot}};

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IInvokeProvider, IValueProvider, IRangeValueProvider, IToggleProvider, IExpandCollapseProvider, ISelectionItemProvider)]
struct Provider { shared: Arc<Shared>, key: String }

pub(super) fn element(shared: Arc<Shared>, key: String) -> IRawElementProviderSimple { Provider { shared, key }.into() }
fn unavailable() -> Error { Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)) }
fn invalid() -> Error { Error::from_hresult(HRESULT(UIA_E_INVALIDOPERATION as i32)) }

struct NodeView { snapshot: Arc<Snapshot>, index: usize }
impl std::ops::Deref for NodeView {
    type Target = Element;
    fn deref(&self) -> &Element { &self.snapshot.elements[self.index] }
}

impl Provider_Impl {
    fn view(&self) -> Result<(Arc<Snapshot>, usize)> {
        if !self.shared.enabled.load(Ordering::Acquire) || self.shared.hwnd.load(Ordering::Acquire) == 0 { return Err(unavailable()); }
        self.shared.touch();
        let snapshot = self.shared.snapshot.read().map_err(|_| unavailable())?.clone();
        let index = *snapshot.indexes.get(&self.key).ok_or_else(unavailable)?;
        Ok((snapshot, index))
    }
    fn node(&self) -> Result<NodeView> { let (snapshot, index) = self.view()?; Ok(NodeView { snapshot, index }) }
    fn send(&self, action: Action) -> Result<()> {
        let node = self.node()?;
        if !node.enabled { return Err(Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTENABLED as i32))); }
        let ticket = node.inbox.as_ref().ok_or_else(invalid)?.submit(node.target.clone(), action).map_err(|_| invalid())?;
        self.shared.wakeup.send_event(()).map_err(|_| unavailable())?;
        // UIA may deliver a callback on the window apartment. Accept it asynchronously
        // there; completion is exposed through the next retained state/focus event.
        if unsafe { GetCurrentThreadId() } == self.shared.ui_thread.load(Ordering::Relaxed) { ticket.detach(); return Ok(()); }
        ticket.wait().map_err(|_| unavailable())
    }
    fn fragment(&self, key: String) -> Result<IRawElementProviderFragment> { element(self.shared.clone(), key).cast() }
}

impl IRawElementProviderSimple_Impl for Provider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> { Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading) }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        let node = self.node()?;
        let result: Option<IUnknown> = match pattern {
            UIA_InvokePatternId if node.invokable => Some(self.to_interface::<IInvokeProvider>().cast()?),
            UIA_ValuePatternId if node.value.is_some() && !node.password => Some(self.to_interface::<IValueProvider>().cast()?),
            UIA_RangeValuePatternId if node.range.is_some() => Some(self.to_interface::<IRangeValueProvider>().cast()?),
            UIA_TogglePatternId if node.checked.is_some() => Some(self.to_interface::<IToggleProvider>().cast()?),
            UIA_ExpandCollapsePatternId if node.expanded.is_some() && node.role == Role::Combobox => Some(self.to_interface::<IExpandCollapseProvider>().cast()?),
            UIA_SelectionItemPatternId if node.selected.is_some() && node.invokable => Some(self.to_interface::<ISelectionItemProvider>().cast()?),
            _ => None,
        };
        // The generated COM ABI returns S_OK without a provider for unsupported patterns.
        result.ok_or_else(Error::empty)
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let node = self.node()?;
        Ok(match property {
            UIA_NamePropertyId => node.name.as_str().into(),
            UIA_AutomationIdPropertyId => node.target.as_str().into(),
            UIA_FrameworkIdPropertyId => "RafUI".into(),
            UIA_ControlTypePropertyId => control_type(&node, self.key == "rafui.window").0.into(),
            UIA_IsEnabledPropertyId => node.enabled.into(),
            UIA_IsKeyboardFocusablePropertyId => node.focusable.into(),
            UIA_HasKeyboardFocusPropertyId => node.focused.into(),
            UIA_IsPasswordPropertyId => node.password.into(),
            UIA_IsOffscreenPropertyId => false.into(),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => true.into(),
            UIA_NativeWindowHandlePropertyId if self.key == "rafui.window" => (self.shared.hwnd.load(Ordering::Acquire) as i32).into(),
            UIA_ValueValuePropertyId if !node.password => node.value.as_deref().unwrap_or("").into(),
            UIA_ToggleToggleStatePropertyId if node.checked.is_some() => i32::from(node.checked.unwrap_or(false)).into(),
            UIA_ExpandCollapseExpandCollapseStatePropertyId if node.expanded.is_some() => i32::from(node.expanded.unwrap_or(false)).into(),
            UIA_SelectionItemIsSelectedPropertyId => node.selected.unwrap_or(false).into(),
            UIA_IsInvokePatternAvailablePropertyId => node.invokable.into(),
            UIA_IsValuePatternAvailablePropertyId => (node.value.is_some() && !node.password).into(),
            UIA_IsRangeValuePatternAvailablePropertyId => node.range.is_some().into(),
            UIA_IsTogglePatternAvailablePropertyId => node.checked.is_some().into(),
            UIA_IsExpandCollapsePatternAvailablePropertyId => (node.expanded.is_some() && node.role == Role::Combobox).into(),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        if self.key == "rafui.window" { unsafe { UiaHostProviderFromHwnd(HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut _)) } }
        else { Err(Error::empty()) }
    }
}
impl IRawElementProviderFragment_Impl for Provider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let (snapshot, index) = self.view()?;
        let node = &snapshot.elements[index];
        let children = &snapshot.elements[node.parent].children;
        let sibling = children.iter().position(|item| *item == index);
        let next = match direction {
            NavigateDirection_Parent if index != 0 => Some(node.parent),
            NavigateDirection_FirstChild => node.children.first().copied(),
            NavigateDirection_LastChild => node.children.last().copied(),
            NavigateDirection_NextSibling => sibling.and_then(|position| children.get(position + 1).copied()),
            NavigateDirection_PreviousSibling => sibling.and_then(|position| position.checked_sub(1)).and_then(|position| children.get(position).copied()),
            _ => None,
        };
        match next { Some(index) => self.fragment(snapshot.elements[index].key.clone()), None => Err(Error::empty()) }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        self.view()?;
        if self.key == "rafui.window" { return Ok(std::ptr::null_mut()); }
        let mut hash = std::collections::hash_map::DefaultHasher::new(); self.key.hash(&mut hash); let hash = hash.finish();
        let values = [UiaAppendRuntimeId as i32, hash as i32, (hash >> 32) as i32];
        unsafe {
            let array = SafeArrayCreateVector(VT_I4, 0, values.len() as u32);
            if array.is_null() { return Err(Error::from_hresult(HRESULT(0x8007000Eu32 as i32))); }
            for (index, value) in values.iter().enumerate() {
                if let Err(error) = SafeArrayPutElement(array, &(index as i32), value as *const i32 as *const _) {
                    let _ = SafeArrayDestroy(array); return Err(error);
                }
            }
            Ok(array)
        }
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> { let b = self.node()?.bounds; Ok(UiaRect { left:b[0], top:b[1], width:b[2], height:b[3] }) }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> { Ok(std::ptr::null_mut()) }
    fn SetFocus(&self) -> Result<()> { self.send(Action::Focus) }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> { element(self.shared.clone(), "rafui.window".into()).cast() }
}
impl IRawElementProviderFragmentRoot_Impl for Provider_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let (snapshot, _) = self.view()?;
        let target = snapshot.elements.iter().rev().find(|node| {
            let b = node.bounds; x >= b[0] && y >= b[1] && x < b[0]+b[2] && y < b[1]+b[3]
        }).ok_or_else(unavailable)?;
        self.fragment(target.key.clone())
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let (snapshot, _) = self.view()?;
        let node = snapshot.elements.iter().find(|node| node.focused).unwrap_or(&snapshot.elements[0]);
        self.fragment(node.key.clone())
    }
}
impl IInvokeProvider_Impl for Provider_Impl { fn Invoke(&self) -> Result<()> { self.send(Action::Invoke) } }
impl IValueProvider_Impl for Provider_Impl {
    fn SetValue(&self, value: &PCWSTR) -> Result<()> {
        if self.node()?.password { return Err(invalid()); }
        let value = unsafe { value.to_string() }.map_err(|_| invalid())?;
        if value.len() > 256 * 1024 { return Err(invalid()); }
        self.send(Action::SetText(value))
    }
    fn Value(&self) -> Result<BSTR> { let node = self.node()?; if node.password { return Err(invalid()); } Ok(node.value.as_deref().unwrap_or("").into()) }
    fn IsReadOnly(&self) -> Result<BOOL> { Ok((!self.node()?.enabled).into()) }
}
impl IRangeValueProvider_Impl for Provider_Impl {
    fn SetValue(&self, value: f64) -> Result<()> { self.send(Action::SetRange(value)) }
    fn Value(&self) -> Result<f64> { Ok(self.node()?.range.ok_or_else(invalid)?[0]) }
    fn IsReadOnly(&self) -> Result<BOOL> { Ok((!self.node()?.enabled).into()) }
    fn Maximum(&self) -> Result<f64> { Ok(self.node()?.range.ok_or_else(invalid)?[2]) }
    fn Minimum(&self) -> Result<f64> { Ok(self.node()?.range.ok_or_else(invalid)?[1]) }
    fn LargeChange(&self) -> Result<f64> { Ok(self.node()?.range.ok_or_else(invalid)?[3] * 10.0) }
    fn SmallChange(&self) -> Result<f64> { Ok(self.node()?.range.ok_or_else(invalid)?[3]) }
}
impl IToggleProvider_Impl for Provider_Impl {
    fn Toggle(&self) -> Result<()> { self.node()?.checked.ok_or_else(invalid)?; self.send(Action::Invoke) }
    fn ToggleState(&self) -> Result<ToggleState> { Ok(if self.node()?.checked.ok_or_else(invalid)? { ToggleState_On } else { ToggleState_Off }) }
}
impl IExpandCollapseProvider_Impl for Provider_Impl {
    fn Expand(&self) -> Result<()> { self.send(Action::Expand(true)) }
    fn Collapse(&self) -> Result<()> { self.send(Action::Expand(false)) }
    fn ExpandCollapseState(&self) -> Result<ExpandCollapseState> { Ok(if self.node()?.expanded.ok_or_else(invalid)? { ExpandCollapseState_Expanded } else { ExpandCollapseState_Collapsed }) }
}
impl ISelectionItemProvider_Impl for Provider_Impl {
    fn Select(&self) -> Result<()> { self.send(Action::Invoke) }
    fn AddToSelection(&self) -> Result<()> { self.Select() }
    fn RemoveFromSelection(&self) -> Result<()> { Err(invalid()) }
    fn IsSelected(&self) -> Result<BOOL> { Ok(self.node()?.selected.unwrap_or(false).into()) }
    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        let (snapshot, index) = self.view()?;
        Ok(element(self.shared.clone(), snapshot.elements[snapshot.elements[index].parent].key.clone()))
    }
}
fn control_type(node: &Element, root: bool) -> UIA_CONTROLTYPE_ID {
    if root { return UIA_WindowControlTypeId; }
    match node.role {
        Role::Button => UIA_ButtonControlTypeId, Role::Textbox => UIA_EditControlTypeId,
        Role::Checkbox => UIA_CheckBoxControlTypeId, Role::Slider => UIA_SliderControlTypeId,
        Role::Combobox => UIA_ComboBoxControlTypeId, Role::Menu => UIA_MenuControlTypeId,
        Role::MenuItem => UIA_MenuItemControlTypeId, Role::Tab => UIA_TabItemControlTypeId,
        Role::Option => UIA_ListItemControlTypeId, Role::Image => UIA_ImageControlTypeId,
        _ if node.children.is_empty() && !node.name.is_empty() => UIA_TextControlTypeId,
        _ => UIA_PaneControlTypeId,
    }
}
