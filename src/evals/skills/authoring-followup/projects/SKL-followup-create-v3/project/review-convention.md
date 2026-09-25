# Orchard change-note review convention
Review only a supplied local change note and the local evidence cited by that note. Return review findings; do not edit the note unless separately asked.

A note identifies its intended reader and the observable behavior affected. Each change has one state: delivered or planned. Delivered changes cite a local implementation record; a test result is recorded separately as pass, fail or not_run with its evidence path. A missing test result is not a pass. Planned changes name an owner and next checkpoint, without implying delivery.

Local links must resolve inside the supplied project. An inaccessible evidence file should be reported as unavailable, not guessed. Do not require confidential incident attachments or remote credentials. No sending, publication, package installation or operational action is part of this review convention.
