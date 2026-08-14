/// UI command-window state machine.
///
/// Keep this as an enum. Do not replace it with a pile of booleans.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CommandWindowState {
    #[default]
    Idle,
    PreparingContext,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

/// Task lifecycle used by the runtime.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_window_starts_idle() {
        assert_eq!(CommandWindowState::default(), CommandWindowState::Idle);
        assert_eq!(TaskStatus::default(), TaskStatus::Pending);
    }
}
