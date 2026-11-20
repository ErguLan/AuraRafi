//! Bounded WebRTC negotiation state machine.
//!
//! The session models the control-plane path around a future media stack. It
//! never performs ICE checks, DTLS handshakes or media work; it only accepts
//! legal control-plane transitions, stores a bounded candidate set and reports
//! deadlines. A media implementation calls [`WebRtcSession::on_connected`]
//! only after it has a real connected path.
//!
//! Offerer path:
//!
//! ```text
//! Idle --on_local_offer--> WaitingAnswer --on_remote_answer--> Gathering
//!      --on_remote_candidate--> Connecting --on_connected--> Connected
//! ```
//!
//! Answerer path:
//!
//! ```text
//! WaitingOffer (new_answerer) --on_remote_offer--> Gathering
//!      --on_remote_candidate--> Connecting --on_connected--> Connected
//! ```
//!
//! The negotiation window is [`NEGOTIATION_TIMEOUT`] (30 s). Once the answer is
//! accepted, the connection window is [`CONNECT_TIMEOUT`] (15 s). A deadline
//! closes the session and emits the timeout event from [`WebRtcSession::tick`].

use super::ice::IceCandidate;
use super::sdp::{validate_answer, SdpType, SessionDescription};
use super::WebRtcError;
use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

/// Maximum candidates retained from the remote peer.
pub const MAX_REMOTE_CANDIDATES: usize = 32;

/// Time allowed to exchange offer and answer.
pub const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Time allowed to reach the connected state after the answer is accepted.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Negotiation lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcState {
    /// No offer has been created or accepted.
    Idle,
    /// Answerer is waiting for a remote offer.
    WaitingOffer,
    /// Offerer is waiting for the remote answer.
    WaitingAnswer,
    /// Answer accepted; ICE candidates are being collected.
    Gathering,
    /// At least one remote candidate is available; the media path is pending.
    Connecting,
    /// A future media layer confirmed the path.
    Connected,
    /// Terminal state.
    Closed,
}

impl WebRtcState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::WaitingOffer => "waiting-offer",
            Self::WaitingAnswer => "waiting-answer",
            Self::Gathering => "gathering",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Closed => "closed",
        }
    }
}

impl fmt::Display for WebRtcState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Notification produced by [`WebRtcSession::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcEvent {
    /// The offer/answer exchange did not complete inside [`NEGOTIATION_TIMEOUT`].
    NegotiationTimeout,
    /// The answer arrived but the path did not connect inside [`CONNECT_TIMEOUT`].
    ConnectTimeout,
    /// A media layer reported a connected path.
    Connected,
    /// The session is closed.
    Closed,
}

#[derive(Debug, Clone, Copy)]
struct Deadline {
    at: Instant,
    event: WebRtcEvent,
}

/// Bounded offer/answer negotiation session.
#[derive(Debug)]
pub struct WebRtcSession {
    state: WebRtcState,
    local_offer: Option<SessionDescription>,
    remote_offer: Option<SessionDescription>,
    remote_answer: Option<SessionDescription>,
    remote_candidates: Vec<IceCandidate>,
    deadline: Option<Deadline>,
    events: VecDeque<WebRtcEvent>,
}

impl Default for WebRtcSession {
    fn default() -> Self {
        Self::new()
    }
}

impl WebRtcSession {
    /// Create an idle offerer-oriented session.
    pub fn new() -> Self {
        Self {
            state: WebRtcState::Idle,
            local_offer: None,
            remote_offer: None,
            remote_answer: None,
            remote_candidates: Vec::new(),
            deadline: None,
            events: VecDeque::new(),
        }
    }

    /// Create a session that waits for a remote offer.
    pub fn new_answerer(now: Instant) -> Self {
        Self {
            state: WebRtcState::WaitingOffer,
            deadline: Some(Deadline {
                at: now + NEGOTIATION_TIMEOUT,
                event: WebRtcEvent::NegotiationTimeout,
            }),
            ..Self::new()
        }
    }

    /// Record the offer produced by the local side.
    pub fn on_local_offer(
        &mut self,
        offer: &SessionDescription,
        now: Instant,
    ) -> Result<(), WebRtcError> {
        self.require_state(WebRtcState::Idle, "on_local_offer")?;
        if offer.sdp_type != SdpType::Offer {
            return Err(WebRtcError::negotiation(
                "on_local_offer requires a description labelled offer",
            ));
        }
        offer.validate()?;
        self.local_offer = Some(offer.clone());
        self.state = WebRtcState::WaitingAnswer;
        self.deadline = Some(Deadline {
            at: now + NEGOTIATION_TIMEOUT,
            event: WebRtcEvent::NegotiationTimeout,
        });
        Ok(())
    }

    /// Accept a remote offer while waiting for one.
    ///
    /// The caller builds and sends the answer separately; the session only
    /// tracks that the offer/answer exchange has started.
    pub fn on_remote_offer(
        &mut self,
        offer: &SessionDescription,
        now: Instant,
    ) -> Result<(), WebRtcError> {
        self.require_state(WebRtcState::WaitingOffer, "on_remote_offer")?;
        if offer.sdp_type != SdpType::Offer {
            return Err(WebRtcError::negotiation(
                "on_remote_offer requires a description labelled offer",
            ));
        }
        offer.validate()?;
        self.remote_offer = Some(offer.clone());
        self.state = if self.remote_candidates.is_empty() {
            WebRtcState::Gathering
        } else {
            WebRtcState::Connecting
        };
        self.deadline = Some(Deadline {
            at: now + CONNECT_TIMEOUT,
            event: WebRtcEvent::ConnectTimeout,
        });
        Ok(())
    }

    /// Accept the answer to the local offer.
    pub fn on_remote_answer(
        &mut self,
        answer: &SessionDescription,
        now: Instant,
    ) -> Result<(), WebRtcError> {
        if self.state == WebRtcState::Closed {
            return Err(WebRtcError::negotiation(
                "on_remote_answer is not valid after the session closed",
            ));
        }
        if self.state != WebRtcState::WaitingAnswer {
            return Err(WebRtcError::negotiation(format!(
                "on_remote_answer is not valid in state {}; a local offer must be sent first",
                self.state
            )));
        }
        if answer.sdp_type != SdpType::Answer {
            return Err(WebRtcError::negotiation(
                "on_remote_answer requires a description labelled answer",
            ));
        }
        let offer = self.local_offer.as_ref().ok_or_else(|| {
            WebRtcError::negotiation("on_remote_answer has no local offer to match")
        })?;
        validate_answer(offer, answer)?;
        self.remote_answer = Some(answer.clone());
        self.state = if self.remote_candidates.is_empty() {
            WebRtcState::Gathering
        } else {
            WebRtcState::Connecting
        };
        self.deadline = Some(Deadline {
            at: now + CONNECT_TIMEOUT,
            event: WebRtcEvent::ConnectTimeout,
        });
        Ok(())
    }

    /// Store one trickled remote candidate.
    ///
    /// Candidates are accepted while waiting for the answer and while
    /// gathering/connecting. Duplicates are ignored. After the session is
    /// connected or closed, late candidates are ignored rather than treated as
    /// errors. Before an offer exists, a candidate is an ordering error.
    pub fn on_remote_candidate(
        &mut self,
        candidate: &IceCandidate,
    ) -> Result<(), WebRtcError> {
        match self.state {
            WebRtcState::Closed | WebRtcState::Connected => return Ok(()),
            WebRtcState::Idle | WebRtcState::WaitingOffer => {
                return Err(WebRtcError::negotiation(format!(
                    "on_remote_candidate is not valid in state {}; no offer is active",
                    self.state
                )));
            }
            WebRtcState::WaitingAnswer
            | WebRtcState::Gathering
            | WebRtcState::Connecting => {}
        }
        candidate.validate()?;
        if self
            .remote_candidates
            .iter()
            .any(|existing| existing == candidate)
        {
            return Ok(());
        }
        if self.remote_candidates.len() >= MAX_REMOTE_CANDIDATES {
            return Err(WebRtcError::limit(format!(
                "the remote candidate list is full ({MAX_REMOTE_CANDIDATES})"
            )));
        }
        self.remote_candidates.push(candidate.clone());
        if self.state == WebRtcState::Gathering {
            self.state = WebRtcState::Connecting;
        }
        Ok(())
    }

    /// Record that a future media layer established the path.
    pub fn on_connected(&mut self) -> Result<(), WebRtcError> {
        match self.state {
            WebRtcState::Connected => return Ok(()),
            WebRtcState::Gathering | WebRtcState::Connecting => {}
            other => {
                return Err(WebRtcError::negotiation(format!(
                    "on_connected is not valid in state {other}; the answer must be accepted first"
                )));
            }
        }
        self.state = WebRtcState::Connected;
        self.deadline = None;
        self.events.push_back(WebRtcEvent::Connected);
        Ok(())
    }

    /// Close the session. This is idempotent.
    pub fn on_closed(&mut self) {
        if self.state == WebRtcState::Closed {
            return;
        }
        self.state = WebRtcState::Closed;
        self.deadline = None;
        self.events.push_back(WebRtcEvent::Closed);
    }

    /// Report at most one pending event and enforce the active deadline.
    ///
    /// Events are reported before the deadline is evaluated. A timeout closes
    /// the session and queues a following [`WebRtcEvent::Closed`] event.
    pub fn tick(&mut self, now: Instant) -> Option<WebRtcEvent> {
        if let Some(event) = self.events.pop_front() {
            return Some(event);
        }
        let deadline = self.deadline?;
        if now < deadline.at {
            return None;
        }
        self.state = WebRtcState::Closed;
        self.deadline = None;
        self.events.push_back(WebRtcEvent::Closed);
        Some(deadline.event)
    }

    pub fn state(&self) -> WebRtcState {
        self.state
    }

    pub fn is_closed(&self) -> bool {
        self.state == WebRtcState::Closed
    }

    pub fn local_offer(&self) -> Option<&SessionDescription> {
        self.local_offer.as_ref()
    }

    pub fn remote_offer(&self) -> Option<&SessionDescription> {
        self.remote_offer.as_ref()
    }

    pub fn remote_answer(&self) -> Option<&SessionDescription> {
        self.remote_answer.as_ref()
    }

    pub fn remote_candidates(&self) -> &[IceCandidate] {
        &self.remote_candidates
    }

    fn require_state(
        &self,
        expected: WebRtcState,
        operation: &str,
    ) -> Result<(), WebRtcError> {
        if self.state == expected {
            return Ok(());
        }
        Err(WebRtcError::negotiation(format!(
            "{operation} is not valid in state {}; expected {}",
            self.state, expected
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webrtc::ice::{IceCandidate, IceCandidateType};
    use crate::webrtc::sdp::{build_answer, build_offer, SdpOfferParams, SessionDescription};
    use crate::webrtc::WebRtcErrorKind;
    use std::time::{Duration, Instant};

    fn test_fingerprint() -> String {
        let mut value = String::with_capacity(95);
        for index in 0..32u8 {
            if index > 0 {
                value.push(':');
            }
            value.push_str(&format!("{index:02X}"));
        }
        value
    }

    fn offer() -> SessionDescription {
        build_offer(&SdpOfferParams::new(
            "session-one",
            "ufrag1",
            "pwd1234",
            test_fingerprint(),
        ))
        .unwrap()
    }

    fn answer(offer: &SessionDescription) -> SessionDescription {
        build_answer(
            offer,
            &SdpOfferParams::new("session-two", "ufrag2", "pwd5678", test_fingerprint()),
        )
        .unwrap()
    }

    fn host_candidate(port: u16) -> IceCandidate {
        IceCandidate {
            foundation: "1".to_string(),
            component: 1,
            transport: "UDP".to_string(),
            priority: 2_130_706_431,
            address: "127.0.0.1".to_string(),
            port,
            typ: IceCandidateType::Host,
            related_address: None,
        }
    }

    #[test]
    fn happy_path_reaches_connected() {
        let now = Instant::now();
        let offer = offer();
        let answer = answer(&offer);
        let mut session = WebRtcSession::new();

        assert_eq!(session.state(), WebRtcState::Idle);
        session.on_local_offer(&offer, now).unwrap();
        assert_eq!(session.state(), WebRtcState::WaitingAnswer);
        session
            .on_remote_answer(&answer, now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(session.state(), WebRtcState::Gathering);

        session.on_remote_candidate(&host_candidate(50_000)).unwrap();
        assert_eq!(session.state(), WebRtcState::Connecting);
        assert_eq!(session.remote_candidates().len(), 1);

        session.on_connected().unwrap();
        assert_eq!(session.state(), WebRtcState::Connected);
        assert_eq!(session.tick(now + Duration::from_secs(2)), Some(WebRtcEvent::Connected));
        assert_eq!(session.tick(now + Duration::from_secs(3)), None);
    }

    #[test]
    fn negotiation_timeout_closes_session() {
        let now = Instant::now();
        let mut session = WebRtcSession::new();
        session.on_local_offer(&offer(), now).unwrap();

        let deadline = now + NEGOTIATION_TIMEOUT;
        assert_eq!(session.tick(deadline - Duration::from_millis(1)), None);
        assert_eq!(
            session.tick(deadline),
            Some(WebRtcEvent::NegotiationTimeout)
        );
        assert_eq!(session.state(), WebRtcState::Closed);
        assert_eq!(
            session.tick(deadline + Duration::from_secs(1)),
            Some(WebRtcEvent::Closed)
        );
    }

    #[test]
    fn connect_timeout_closes_session() {
        let now = Instant::now();
        let offer = offer();
        let mut session = WebRtcSession::new();
        session.on_local_offer(&offer, now).unwrap();
        let answered_at = now + Duration::from_secs(1);
        session.on_remote_answer(&answer(&offer), answered_at).unwrap();
        assert_eq!(session.state(), WebRtcState::Gathering);

        let deadline = answered_at + CONNECT_TIMEOUT;
        assert_eq!(session.tick(deadline - Duration::from_millis(1)), None);
        assert_eq!(session.tick(deadline), Some(WebRtcEvent::ConnectTimeout));
        assert_eq!(session.state(), WebRtcState::Closed);
    }

    #[test]
    fn answer_before_offer_is_rejected() {
        let offer = offer();
        let mut session = WebRtcSession::new();
        let error = session
            .on_remote_answer(&answer(&offer), Instant::now())
            .unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Negotiation);
        assert_eq!(session.state(), WebRtcState::Idle);
        assert!(session.remote_answer().is_none());
    }

    #[test]
    fn candidates_are_ignored_after_close() {
        let mut session = WebRtcSession::new();
        session.on_closed();
        assert!(session.is_closed());
        session.on_remote_candidate(&host_candidate(50_000)).unwrap();
        assert_eq!(session.state(), WebRtcState::Closed);
        assert!(session.remote_candidates().is_empty());
    }

    #[test]
    fn remote_candidate_list_is_bounded() {
        let now = Instant::now();
        let mut session = WebRtcSession::new();
        session.on_local_offer(&offer(), now).unwrap();
        for index in 0..MAX_REMOTE_CANDIDATES {
            session
                .on_remote_candidate(&host_candidate(50_000 + index as u16))
                .unwrap();
        }
        assert_eq!(session.remote_candidates().len(), MAX_REMOTE_CANDIDATES);
        let error = session
            .on_remote_candidate(&host_candidate(51_000))
            .unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Limit);
        assert_eq!(session.remote_candidates().len(), MAX_REMOTE_CANDIDATES);
    }

    #[test]
    fn answerer_flow_reaches_connected() {
        let now = Instant::now();
        let mut session = WebRtcSession::new_answerer(now);
        assert_eq!(session.state(), WebRtcState::WaitingOffer);
        session
            .on_remote_offer(&offer(), now + Duration::from_secs(1))
            .unwrap();
        assert_eq!(session.state(), WebRtcState::Gathering);
        session.on_remote_candidate(&host_candidate(50_000)).unwrap();
        assert_eq!(session.state(), WebRtcState::Connecting);
        session.on_connected().unwrap();
        assert_eq!(session.state(), WebRtcState::Connected);
        assert_eq!(
            session.tick(now + Duration::from_secs(2)),
            Some(WebRtcEvent::Connected)
        );
    }
}
