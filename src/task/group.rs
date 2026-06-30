use std::collections::HashMap;

/// Unique identifier for a group.
pub type GroupId = u64;

/// A logical grouping of related tasks.
///
/// Groups form a tree via the optional `parent` pointer, enabling
/// "smart grouping" -- e.g. an outer "Import" group containing inner
/// "Decoding", "Thumbnail", "Metadata" groups.
#[derive(Debug, Clone)]
pub struct TaskGroup {
    pub(crate) id: GroupId,
    pub(crate) name: String,
    pub(crate) parent: Option<GroupId>,
    /// UI hint: is the group collapsed in the task tree?
    pub(crate) collapsed: bool,
}

impl TaskGroup {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            parent: None,
            collapsed: false,
        }
    }

    pub fn parent(mut self, parent: GroupId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent_id(&self) -> Option<GroupId> {
        self.parent
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_collapsed(&mut self, c: bool) {
        self.collapsed = c;
    }
}

/// Tree of groups, used to compute aggregate progress / counts.
#[derive(Debug, Default)]
pub(crate) struct GroupTree {
    pub(crate) by_id: HashMap<GroupId, TaskGroup>,
    pub(crate) children: HashMap<GroupId, Vec<GroupId>>,
}

impl GroupTree {
    pub fn insert(&mut self, group: TaskGroup) -> GroupId {
        let id = group.id;
        if let Some(parent) = group.parent {
            self.children.entry(parent).or_default().push(id);
        }
        self.by_id.insert(id, group);
        id
    }

    pub fn children_of(&self, id: GroupId) -> &[GroupId] {
        self.children.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Walk the subtree rooted at `root` in depth-first pre-order.
    pub fn walk(&self, root: GroupId) -> Vec<GroupId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            for &child in self.children_of(id).iter().rev() {
                stack.push(child);
            }
        }
        out
    }
}
