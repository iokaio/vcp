// SPDX-License-Identifier: Apache-2.0
use super::{
    budget::{Budget, Limits},
    proposal::{Disposition as ProposalDisposition, Proposal},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{
    AuthorityRevision, DeletionEpoch, Revision, SteeringRevision, TaskId, Timestamp,
    VerificationId, Watermark,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub root: TaskId,
    pub task: TaskId,
    pub steering: SteeringRevision,
    pub task_revision: Revision,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub input_digest: String,
    pub pattern_digest: String,
    pub watermark: Watermark,
    pub deadline: Timestamp,
}
impl Input {
    pub fn validate(&self) -> Result<()> {
        if !vcp_domain::accounting::valid_hash(&self.input_digest)
            || !vcp_domain::accounting::valid_hash(&self.pattern_digest)
            || self.deadline == Timestamp::ZERO
        {
            return Err(Error::Invalid("input"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub id: u32,
    pub key: String,
    pub input: Input,
    pub deadline: Timestamp,
    pub reserved_steps: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Completed,
    Interrupted,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub work: Work,
    pub status: AttemptStatus,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    Queued,
    Coalesced,
    Duplicate,
    Older,
    Disabled,
    Paused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub root: TaskId,
    pub enabled: bool,
    pub cursor: Watermark,
    pub queued: Option<Input>,
    pub attempts: Vec<Attempt>,
    pub proposals: Vec<Proposal>,
    pub limits: Limits,
    pub budget: Budget,
    pub last_started: Option<Timestamp>,
    pub coalesced: u64,
    pub duplicates: u64,
}
impl State {
    pub fn new(root: TaskId, limits: Limits) -> Result<Self> {
        limits.validate()?;
        Ok(Self {
            version: 1,
            root,
            enabled: false,
            cursor: Watermark::ZERO,
            queued: None,
            attempts: vec![],
            proposals: vec![],
            limits,
            budget: Budget::default(),
            last_started: None,
            coalesced: 0,
            duplicates: 0,
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        if self.version != 1
            || self.attempts.len() > self.limits.max_attempts as usize
            || self.proposals.len() > self.attempts.len()
            || self.budget.attempts as usize != self.attempts.len()
            || self.budget.reserved_steps > self.limits.max_total_steps
        {
            return Err(Error::Invalid("state bounds"));
        }
        let mut keys = BTreeSet::new();
        let mut total = 0u64;
        let mut running = 0;
        for (index, attempt) in self.attempts.iter().enumerate() {
            let work = &attempt.work;
            work.input.validate()?;
            if work.id as usize != index + 1
                || work.input.root != self.root
                || work.key != super::dedup::key(&work.input)?
                || !keys.insert(&work.key)
                || work.reserved_steps != self.limits.steps_per_attempt
                || work.deadline > work.input.deadline
                || work.input.watermark > self.cursor
            {
                return Err(Error::Invalid("attempt"));
            }
            total += u64::from(work.reserved_steps);
            if attempt.status == AttemptStatus::Running {
                running += 1;
            }
        }
        if running > 1 || total != self.budget.reserved_steps {
            return Err(Error::Invalid("budget or concurrency"));
        }
        let mut proposals = BTreeSet::new();
        for proposal in &self.proposals {
            proposal.validate()?;
            if proposal.input.root != self.root
                || !proposals.insert(&proposal.key)
                || !self.attempts.iter().any(|a| {
                    let mut expected = a.work.input.clone();
                    expected.pattern_digest = proposal.input.pattern_digest.clone();
                    a.work.id == proposal.attempt
                        && a.status == AttemptStatus::Completed
                        && expected == proposal.input
                })
            {
                return Err(Error::Invalid("proposal attempt"));
            }
        }
        if let Some(input) = &self.queued {
            input.validate()?;
            if input.root != self.root
                || input.watermark > self.cursor
                || keys.contains(&super::dedup::key(input)?)
            {
                return Err(Error::Invalid("queued input"));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::Invalid("state encoding"))?
            .len()
            > 2 * 1024 * 1024
        {
            return Err(Error::Invalid("state bytes"));
        }
        Ok(())
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<()> {
        self.validate()?;
        self.enabled = enabled;
        if !enabled {
            self.queued = None;
            for proposal in &mut self.proposals {
                proposal.disposition = ProposalDisposition::Historical;
            }
        }
        Ok(())
    }
    /// A single coalescing slot; queued delivery never creates an admission.
    pub fn enqueue(&mut self, input: Input, parent_running: bool) -> Result<Disposition> {
        self.validate()?;
        input.validate()?;
        if input.root != self.root {
            return Err(Error::Invalid("root mismatch"));
        }
        if !self.enabled {
            return Ok(Disposition::Disabled);
        }
        if !parent_running {
            self.queued = None;
            return Ok(Disposition::Paused);
        }
        if input.watermark < self.cursor {
            return Ok(Disposition::Older);
        }
        self.cursor = input.watermark;
        let key = super::dedup::key(&input)?;
        if self.attempts.iter().any(|a| a.work.key == key)
            || self
                .queued
                .as_ref()
                .is_some_and(|q| super::dedup::key(q).ok().as_ref() == Some(&key))
        {
            self.duplicates = self.duplicates.saturating_add(1);
            return Ok(Disposition::Duplicate);
        }
        let replaced = self.queued.replace(input).is_some();
        if replaced {
            self.coalesced = self.coalesced.saturating_add(1);
            Ok(Disposition::Coalesced)
        } else {
            Ok(Disposition::Queued)
        }
    }
    /// The caller persists the resulting Running attempt before any computation.
    pub fn begin(&mut self, now: Timestamp, parent_running: bool) -> Result<Option<Work>> {
        self.validate()?;
        if !self.enabled || !parent_running {
            return Ok(None);
        }
        if self
            .attempts
            .iter()
            .any(|a| a.status == AttemptStatus::Running)
            || !super::debounce::ready(now, self.last_started, self.limits.debounce_ms)
        {
            return Ok(None);
        }
        let Some(input) = self.queued.as_ref() else {
            return Ok(None);
        };
        if now >= input.deadline {
            self.queued = None;
            return Ok(None);
        }
        let deadline = Timestamp::new(
            now.get()
                .saturating_add(self.limits.deadline_ms)
                .min(input.deadline.get()),
        );
        let key = super::dedup::key(input)?;
        self.budget.reserve(&self.limits)?;
        let work = Work {
            id: self.budget.attempts,
            key,
            input: input.clone(),
            deadline,
            reserved_steps: self.limits.steps_per_attempt,
        };
        self.queued = None;
        self.last_started = Some(now);
        self.attempts.push(Attempt {
            work: work.clone(),
            status: AttemptStatus::Running,
        });
        Ok(Some(work))
    }
    /// Admission uses the digest of the full observed verification projection
    /// in Input.pattern_digest. The completed proposal substitutes the computed
    /// exact pattern digest, deduplicating facts across later observations.
    pub fn complete_fact(
        &mut self,
        work: &Work,
        current: &Input,
        parent_running: bool,
        now: Timestamp,
        pattern_digest: String,
        source_evidence: String,
        verifications: Vec<VerificationId>,
    ) -> Result<Option<Proposal>> {
        self.validate()?;
        current.validate()?;
        let index = self
            .attempts
            .iter()
            .position(|a| a.work == *work && a.status == AttemptStatus::Running)
            .ok_or(Error::Stale)?;
        if now >= work.deadline {
            self.attempts[index].status = AttemptStatus::Expired;
            return Err(Error::Stale);
        }
        let disposition = if self.enabled
            && parent_running
            && super::dedup::key(current)? == work.key
            && now < current.deadline
        {
            ProposalDisposition::Current
        } else {
            ProposalDisposition::Historical
        };
        let mut input = work.input.clone();
        input.pattern_digest = pattern_digest;
        let proposal = Proposal {
            attempt: work.id,
            key: super::dedup::key(&input)?,
            input,
            source_evidence,
            verifications,
            disposition,
        };
        proposal.validate()?;
        if self.proposals.iter().any(|p| p.key == proposal.key) {
            self.attempts[index].status = AttemptStatus::Completed;
            self.duplicates = self.duplicates.saturating_add(1);
            return Ok(None);
        }
        let mut candidate = self.clone();
        candidate.attempts[index].status = AttemptStatus::Completed;
        candidate.proposals.push(proposal.clone());
        candidate.validate()?;
        *self = candidate;
        Ok(Some(proposal))
    }
    pub fn complete_without_proposal(&mut self, work: &Work, now: Timestamp) -> Result<()> {
        self.validate()?;
        let attempt = self
            .attempts
            .iter_mut()
            .find(|a| a.work == *work && a.status == AttemptStatus::Running)
            .ok_or(Error::Stale)?;
        attempt.status = if now >= work.deadline {
            AttemptStatus::Expired
        } else {
            AttemptStatus::Completed
        };
        Ok(())
    }
    pub fn advance_cursor(&mut self, watermark: Watermark) -> Result<()> {
        self.validate()?;
        if watermark < self.cursor {
            return Err(Error::Stale);
        }
        self.cursor = watermark;
        Ok(())
    }
    /// Explicit owner loss reconciliation. It never requeues an interrupted key,
    /// restores enablement, refunds spent local work, or authorizes model replay.
    pub fn recover(&mut self) -> Result<()> {
        self.validate()?;
        self.enabled = false;
        self.queued = None;
        for proposal in &mut self.proposals {
            proposal.disposition = ProposalDisposition::Historical;
        }
        for attempt in &mut self.attempts {
            if attempt.status == AttemptStatus::Running {
                attempt.status = AttemptStatus::Interrupted;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(root: &TaskId, n: u64) -> Input {
        Input {
            root: root.clone(),
            task: root.clone(),
            steering: SteeringRevision::ZERO,
            task_revision: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            input_digest: vcp_protocol::digest_bytes(b"input"),
            pattern_digest: vcp_protocol::digest_bytes(&n.to_le_bytes()),
            watermark: Watermark::new(n),
            deadline: Timestamp::new(10000),
        }
    }
    fn state() -> State {
        let mut s = State::new(
            TaskId::new(),
            Limits {
                debounce_ms: 0,
                ..Limits::default()
            },
        )
        .unwrap();
        s.set_enabled(true).unwrap();
        s
    }
    #[test]
    fn disabled_pause_storm_and_duplicate_are_bounded() {
        let mut s = state();
        s.set_enabled(false).unwrap();
        let first = input(&s.root, 1);
        assert_eq!(
            s.enqueue(first.clone(), true).unwrap(),
            Disposition::Disabled
        );
        assert_eq!(s.budget.attempts, 0);
        s.set_enabled(true).unwrap();
        assert_eq!(
            s.enqueue(first.clone(), false).unwrap(),
            Disposition::Paused
        );
        for n in 1..1000 {
            s.enqueue(input(&s.root, n), true).unwrap();
        }
        assert_eq!(s.coalesced, 998);
        assert_eq!(s.queued.as_ref().unwrap().watermark, Watermark::new(999));
        let work = s.begin(Timestamp::new(1), true).unwrap().unwrap();
        assert_eq!(
            s.enqueue(work.input.clone(), true).unwrap(),
            Disposition::Duplicate
        );
        assert!(s.begin(Timestamp::new(2), true).unwrap().is_none());
        assert_eq!(s.budget.attempts, 1);
        s.validate().unwrap();
    }
    #[test]
    fn stale_completion_remains_historical_and_cannot_repeat() {
        let mut s = state();
        let first = input(&s.root, 1);
        s.enqueue(first.clone(), true).unwrap();
        let work = s.begin(Timestamp::new(1), true).unwrap().unwrap();
        let mut current = first.clone();
        current.steering = SteeringRevision::new(1);
        let p = s
            .complete_fact(
                &work,
                &current,
                true,
                Timestamp::new(2),
                vcp_protocol::digest_bytes(b"pattern"),
                "evidence".into(),
                vec![
                    VerificationId::new(),
                    VerificationId::new(),
                    VerificationId::new(),
                ],
            )
            .unwrap()
            .unwrap();
        assert_eq!(p.disposition, ProposalDisposition::Historical);
        assert!(s
            .complete_without_proposal(&work, Timestamp::new(2))
            .is_err());
        s.validate().unwrap();
    }
    #[test]
    fn same_fact_on_later_input_has_one_proposal() {
        let mut s = state();
        for n in 1..=2 {
            let current = input(&s.root, n);
            s.enqueue(current.clone(), true).unwrap();
            let work = s.begin(Timestamp::new(n), true).unwrap().unwrap();
            let p = s
                .complete_fact(
                    &work,
                    &current,
                    true,
                    Timestamp::new(n + 1),
                    vcp_protocol::digest_bytes(b"same exact cycle"),
                    "evidence".into(),
                    vec![
                        VerificationId::new(),
                        VerificationId::new(),
                        VerificationId::new(),
                    ],
                )
                .unwrap();
            assert_eq!(p.is_some(), n == 1);
        }
        assert_eq!(s.proposals.len(), 1);
        assert_eq!(s.budget.attempts, 2);
        s.validate().unwrap();
    }
    #[test]
    fn deadline_and_root_caps_do_not_refund_or_dispatch() {
        let mut s = state();
        s.limits.max_attempts = 1;
        let first = input(&s.root, 1);
        s.enqueue(first.clone(), true).unwrap();
        let work = s.begin(Timestamp::new(1), true).unwrap().unwrap();
        assert!(s
            .complete_fact(
                &work,
                &first,
                true,
                work.deadline,
                vcp_protocol::digest_bytes(b"pattern"),
                "evidence".into(),
                vec![
                    VerificationId::new(),
                    VerificationId::new(),
                    VerificationId::new()
                ]
            )
            .is_err());
        assert_eq!(s.attempts[0].status, AttemptStatus::Expired);
        s.enqueue(input(&s.root, 2), true).unwrap();
        assert!(matches!(
            s.begin(Timestamp::new(3000), true),
            Err(Error::Capacity)
        ));
        assert_eq!(s.budget.reserved_steps, 4096);
    }
    #[test]
    fn restart_requires_enable_and_never_replays_interrupted_key() {
        let mut s = state();
        let first = input(&s.root, 1);
        s.enqueue(first.clone(), true).unwrap();
        s.begin(Timestamp::new(1), true).unwrap().unwrap();
        let bytes = serde_json::to_vec(&s).unwrap();
        let mut restored: State = serde_json::from_slice(&bytes).unwrap();
        restored.recover().unwrap();
        assert!(!restored.enabled);
        assert_eq!(restored.attempts[0].status, AttemptStatus::Interrupted);
        restored.set_enabled(true).unwrap();
        assert_eq!(
            restored.enqueue(first, true).unwrap(),
            Disposition::Duplicate
        );
        assert!(restored.begin(Timestamp::new(3), true).unwrap().is_none());
        assert_eq!(restored.budget.attempts, 1);
    }
    #[test]
    fn negatives_debounce_and_pause_never_create_fact() {
        let mut s = state();
        s.limits.debounce_ms = 100;
        let first = input(&s.root, 1);
        s.enqueue(first, true).unwrap();
        let work = s.begin(Timestamp::new(1), true).unwrap().unwrap();
        s.complete_without_proposal(&work, Timestamp::new(2))
            .unwrap();
        s.enqueue(input(&s.root, 2), true).unwrap();
        assert!(s.begin(Timestamp::new(3), true).unwrap().is_none());
        assert!(s.begin(Timestamp::new(200), false).unwrap().is_none());
        let work = s.begin(Timestamp::new(200), true).unwrap().unwrap();
        let current = work.input.clone();
        let p = s
            .complete_fact(
                &work,
                &current,
                false,
                Timestamp::new(201),
                vcp_protocol::digest_bytes(b"fact"),
                "evidence".into(),
                vec![
                    VerificationId::new(),
                    VerificationId::new(),
                    VerificationId::new(),
                ],
            )
            .unwrap()
            .unwrap();
        assert_eq!(p.disposition, ProposalDisposition::Historical);
    }
    #[test]
    fn fewer_than_three_verifications_cannot_form_a_repetition_fact() {
        let mut state = state();
        let current = input(&state.root, 1);
        state.enqueue(current.clone(), true).unwrap();
        let work = state.begin(Timestamp::new(1), true).unwrap().unwrap();
        assert!(state
            .complete_fact(
                &work,
                &current,
                true,
                Timestamp::new(2),
                vcp_protocol::digest_bytes(b"pattern"),
                "evidence".into(),
                vec![VerificationId::new()],
            )
            .is_err());
        assert!(state.proposals.is_empty());
        assert_eq!(state.attempts[0].status, AttemptStatus::Running);
        state
            .complete_without_proposal(&work, Timestamp::new(2))
            .unwrap();
    }
    #[test]
    fn forged_state_and_root_fail_validation() {
        let mut s = state();
        let mut other = input(&TaskId::new(), 1);
        assert!(s.enqueue(other.clone(), true).is_err());
        other.root = s.root.clone();
        other.pattern_digest = "not-a-hash".into();
        assert!(s.enqueue(other, true).is_err());
        s.budget.attempts = 1;
        assert!(s.validate().is_err());
        let mut limits = Limits::default();
        limits.max_attempts = 65;
        assert!(State::new(TaskId::new(), limits).is_err());
    }
}
