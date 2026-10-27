# Owned COW undo hypothesis

Previous COW rollback cloned a shared Row before applying RowDelta, forcing a map detach even without any outstanding client Handle. New Store helper takes ownership of the current slot, captures only id/uri cells for identity maintenance, removes current secondary/FTS entries, applies delta, installs the restored row before map fallback/rebuild and restores indexes. Outstanding Handles still cause Arc::make_mut to detach. Rows and Mixed undo both use this helper.

Full debug suite444passed/2ignored, including new held-UPDATE-result rollback isolation regression and all previous model/index/transaction/crash tests. No fresh performance claim yet. The previous failed warm rollback pair remains preserved; this hypothesis does not establish its cause or repeat the unchanged candidate.
