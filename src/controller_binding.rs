use serde::{Deserialize, Serialize};

/// A stable controller model identity, independent of USB paths or gamepad index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerBinding {
    pub id: String,
    pub name: String,
}
