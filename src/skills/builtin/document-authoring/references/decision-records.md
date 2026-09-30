# Decision records and proposals

VCP-authored reference. Use an architecture decision record (ADR) to record a
decision that has been made. Use a decision proposal to ask for a decision that
has not been made yet.

## Architecture decision record

Follow the project's ADR template and numbering when one exists. Otherwise use:

- **Title and number:** a short, specific name for the decision.
- **Status:** proposed, accepted, deprecated or superseded, with the date.
- **Context:** the problem, constraints and forces at the time, with links to
  evidence. State what was unknown.
- **Decision:** what was chosen, stated as a direct commitment. Name the main
  alternatives considered and why they were not chosen.
- **Consequences:** what becomes easier, what becomes harder, new obligations,
  risks and follow-up work. Include negative consequences.
- **Supersession:** links to records this one supersedes or is superseded by.

Preserve history. An accepted ADR records what was decided and why at that
time. When the decision changes, write a new record that supersedes it and
update only the old record's status and supersession link. Do not rewrite the
old context or decision to match the new one.

Record only decisions that were actually made. If an approver, date or outcome
is unknown, mark it rather than inventing it.

## Decision proposal

Use when a reader must choose between materially different options.

- **Problem:** the decision needed, why now and the cost of not deciding.
- **Options:** two or more real options, each with its trade-offs: benefits,
  costs, risks and evidence. Include "do nothing" when it is viable.
- **Recommendation:** the preferred option and the reasons it wins over the
  others. Keep the analysis and the recommendation distinguishable.
- **What depends on the decision:** the work, people or dates blocked until it
  is made, and the deadline if there is one.
- **Reversibility:** how costly it is to change course later, and what would
  cause the decision to be revisited.

Put the requested decision in the first lines so a reader can respond without
reading the whole proposal. Separate verified facts from estimates and
assumptions. When the decision is made, record it as an ADR or in the project's
equivalent record.
