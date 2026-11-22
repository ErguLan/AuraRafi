use std::{mem::MaybeUninit, ptr};

use windows_sys::Win32::System::Performance::*;

// Formatting flags from pdh.h that the Windows bindings omit.
const FORMAT_NO_SCALE: PDH_FMT = 0x1000;
const FORMAT_NO_PERCENT_CAP: PDH_FMT = 0x8000;

pub(super) fn sample() -> (Option<u64>, Option<u64>) {
    let mut query = ptr::null_mut();
    if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut query) } != 0 {
        return (None, None);
    }
    let query = Query(query);
    let shared = query.counter("Shared Usage");
    let dedicated = query.counter("Dedicated Usage");
    if unsafe { PdhCollectQueryData(query.0) } != 0 {
        return (None, None);
    }
    let prefix: Vec<u16> = format!("pid_{}_", std::process::id())
        .encode_utf16()
        .collect();
    (
        shared.and_then(|counter| read(counter, &prefix)),
        dedicated.and_then(|counter| read(counter, &prefix)),
    )
}

struct Query(PDH_HQUERY);

impl Query {
    fn counter(&self, name: &str) -> Option<PDH_HCOUNTER> {
        let path: Vec<u16> = format!(r"\GPU Process Memory(*)\{name}")
            .encode_utf16()
            .chain([0])
            .collect();
        let mut counter = ptr::null_mut();
        let status = unsafe { PdhAddEnglishCounterW(self.0, path.as_ptr(), 0, &mut counter) };
        (status == 0).then_some(counter)
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.0);
        }
    }
}

fn read(counter: PDH_HCOUNTER, prefix: &[u16]) -> Option<u64> {
    let mut bytes = 0;
    let mut count = 0;
    let format = PDH_FMT_LARGE | FORMAT_NO_SCALE | FORMAT_NO_PERCENT_CAP;
    if unsafe {
        PdhGetFormattedCounterArrayW(counter, format, &mut bytes, &mut count, ptr::null_mut())
    } != PDH_MORE_DATA
    {
        return None;
    }
    let slots = (bytes as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
    // Match the native alignment and leave room for the names following the array.
    let mut storage = vec![MaybeUninit::<PDH_FMT_COUNTERVALUE_ITEM_W>::uninit(); slots];
    let start = storage.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    if unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut bytes, &mut count, start) } != 0
        || count as usize > slots
    {
        return None;
    }
    let end = (start as usize).checked_add(bytes as usize)?;
    let mut found = false;
    let mut total = 0u64;
    for item in unsafe { std::slice::from_raw_parts(start, count as usize) } {
        let address = item.szName as usize;
        if address < start as usize
            || address.checked_add(std::mem::size_of_val(prefix))? > end
            || address % align_of::<u16>() != 0
        {
            continue;
        }
        let matches = prefix
            .iter()
            .enumerate()
            .all(|(i, &expected)| unsafe { item.szName.add(i).read() } == expected);
        if matches
            && matches!(
                item.FmtValue.CStatus,
                PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
            )
        {
            found = true;
            total =
                total.saturating_add(unsafe { item.FmtValue.Anonymous.largeValue }.max(0) as u64);
        }
    }
    found.then_some(total)
}
