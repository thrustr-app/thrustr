use crate::component::Error;
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum Status {
    Inactive,
    Initializing,
    Active,
    #[strum(to_string = "initialization error")]
    InitError(Error),
    Unauthenticated,
    Error(Error),
}

#[derive(Debug)]
pub enum StatusEvent {
    InitStarted,
    InitSucceeded,
    InitFailed(Error),
    LoggedIn,
    LoggedOut,
    ConfigSaved,
    OperationFailed(Error),
}

impl Status {
    pub fn is_inactive(&self) -> bool {
        matches!(self, Self::Inactive)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_initializing(&self) -> bool {
        matches!(self, Self::Initializing)
    }

    pub fn is_init_error(&self) -> bool {
        matches!(self, Self::InitError(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub fn is_any_error(&self) -> bool {
        matches!(self, Self::InitError(_) | Self::Error(_))
    }

    pub fn can_init(&self) -> bool {
        matches!(self, Self::Inactive | Self::InitError(_))
    }

    pub fn can_login(&self) -> bool {
        matches!(
            self,
            Self::Unauthenticated | Self::Error(Error::Auth(_)) | Self::InitError(Error::Auth(_))
        )
    }

    pub fn can_logout(&self) -> bool {
        matches!(
            self,
            Self::Active
                | Self::Error(Error::Config(_) | Error::Other(_))
                | Self::InitError(Error::Config(_) | Error::Other(_))
        )
    }

    pub fn can_configure(&self) -> bool {
        matches!(
            self,
            Self::Active
                | Self::Unauthenticated
                | Self::Error(Error::Config(_))
                | Self::InitError(Error::Config(_))
        )
    }

    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::InitError(e) | Self::Error(e) => Some(e.to_string()),
            _ => None,
        }
    }

    pub fn apply(&self, event: StatusEvent) -> Option<Status> {
        match (self, event) {
            (s, StatusEvent::InitStarted) if s.can_init() => Some(Self::Initializing),
            (Self::Initializing, StatusEvent::InitSucceeded) => Some(Self::Active),
            (Self::Initializing, StatusEvent::InitFailed(e)) => Some(Self::InitError(e)),
            // Login lands in Inactive so the component re-inits.
            (s, StatusEvent::LoggedIn) if s.can_login() => Some(Self::Inactive),
            (s, StatusEvent::LoggedOut) if s.can_logout() => Some(Self::Unauthenticated),
            (Self::Active, StatusEvent::ConfigSaved) => Some(Self::Active),
            (Self::Unauthenticated, StatusEvent::ConfigSaved) => Some(Self::Unauthenticated),
            (s, StatusEvent::ConfigSaved) if s.can_configure() => Some(Self::Inactive),
            // Only auth and config failures demote, anything else is transient.
            (Self::Active, StatusEvent::OperationFailed(e)) => Some(match e {
                Error::Auth(_) | Error::Config(_) => Self::Error(e),
                Error::Other(_) => Self::Active,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_error() -> Error {
        Error::Auth("expired".into())
    }

    fn config_error() -> Error {
        Error::Config("bad path".into())
    }

    fn other_error() -> Error {
        Error::Other("network".into())
    }

    #[track_caller]
    fn check_apply(status: Status, event: StatusEvent, expected: Option<Status>) {
        assert_eq!(status.apply(event), expected);
    }

    struct AllowedActions {
        can_init: bool,
        can_login: bool,
        can_logout: bool,
        can_configure: bool,
    }

    #[track_caller]
    fn check_allowed_actions(status: Status, expected: AllowedActions) {
        assert_eq!(status.can_init(), expected.can_init, "can_init");
        assert_eq!(status.can_login(), expected.can_login, "can_login");
        assert_eq!(status.can_logout(), expected.can_logout, "can_logout");
        assert_eq!(
            status.can_configure(),
            expected.can_configure,
            "can_configure"
        );
    }

    #[test]
    fn allowed_actions_are_determined_by_status() {
        check_allowed_actions(
            Status::Inactive,
            AllowedActions {
                can_init: true,
                can_login: false,
                can_logout: false,
                can_configure: false,
            },
        );
        check_allowed_actions(
            Status::Initializing,
            AllowedActions {
                can_init: false,
                can_login: false,
                can_logout: false,
                can_configure: false,
            },
        );
        check_allowed_actions(
            Status::Active,
            AllowedActions {
                can_init: false,
                can_login: false,
                can_logout: true,
                can_configure: true,
            },
        );
        check_allowed_actions(
            Status::Unauthenticated,
            AllowedActions {
                can_init: false,
                can_login: true,
                can_logout: false,
                can_configure: true,
            },
        );
        check_allowed_actions(
            Status::InitError(auth_error()),
            AllowedActions {
                can_init: true,
                can_login: true,
                can_logout: false,
                can_configure: false,
            },
        );
        check_allowed_actions(
            Status::InitError(config_error()),
            AllowedActions {
                can_init: true,
                can_login: false,
                can_logout: true,
                can_configure: true,
            },
        );
        check_allowed_actions(
            Status::InitError(other_error()),
            AllowedActions {
                can_init: true,
                can_login: false,
                can_logout: true,
                can_configure: false,
            },
        );
        check_allowed_actions(
            Status::Error(auth_error()),
            AllowedActions {
                can_init: false,
                can_login: true,
                can_logout: false,
                can_configure: false,
            },
        );
        check_allowed_actions(
            Status::Error(config_error()),
            AllowedActions {
                can_init: false,
                can_login: false,
                can_logout: true,
                can_configure: true,
            },
        );
        check_allowed_actions(
            Status::Error(other_error()),
            AllowedActions {
                can_init: false,
                can_login: false,
                can_logout: true,
                can_configure: false,
            },
        );
    }

    #[test]
    fn error_message_present_only_for_error_states() {
        for status in [
            Status::Inactive,
            Status::Initializing,
            Status::Active,
            Status::Unauthenticated,
        ] {
            assert_eq!(status.error_message(), None);
        }
        assert_eq!(
            Status::Error(auth_error()).error_message(),
            Some(auth_error().to_string())
        );
        assert_eq!(
            Status::InitError(config_error()).error_message(),
            Some(config_error().to_string())
        );
    }

    #[test]
    fn is_any_error_covers_both_error_states() {
        assert!(Status::Error(other_error()).is_any_error());
        assert!(Status::InitError(other_error()).is_any_error());
        assert!(!Status::Active.is_any_error());
    }

    #[test]
    fn init_advances_from_inactive_to_active() {
        check_apply(
            Status::Inactive,
            StatusEvent::InitStarted,
            Some(Status::Initializing),
        );
        check_apply(
            Status::Initializing,
            StatusEvent::InitSucceeded,
            Some(Status::Active),
        );
    }

    #[test]
    fn init_can_be_retried_after_any_init_error() {
        for error in [auth_error(), config_error(), other_error()] {
            check_apply(
                Status::InitError(error),
                StatusEvent::InitStarted,
                Some(Status::Initializing),
            );
        }
    }

    #[test]
    fn init_failure_records_the_cause() {
        for error in [auth_error(), config_error(), other_error()] {
            check_apply(
                Status::Initializing,
                StatusEvent::InitFailed(error.clone()),
                Some(Status::InitError(error)),
            );
        }
    }

    #[test]
    fn login_lands_inactive_and_ready_to_reinit() {
        for prior in [
            Status::Unauthenticated,
            Status::Error(auth_error()),
            Status::InitError(auth_error()),
        ] {
            check_apply(prior, StatusEvent::LoggedIn, Some(Status::Inactive));
        }
    }

    #[test]
    fn logout_lands_unauthenticatede() {
        for prior in [
            Status::Active,
            Status::Error(config_error()),
            Status::Error(other_error()),
            Status::InitError(config_error()),
            Status::InitError(other_error()),
        ] {
            check_apply(prior, StatusEvent::LoggedOut, Some(Status::Unauthenticated));
        }
    }

    #[test]
    fn auth_operation_failure_demotes_active_to_error() {
        check_apply(
            Status::Active,
            StatusEvent::OperationFailed(auth_error()),
            Some(Status::Error(auth_error())),
        );
    }

    #[test]
    fn config_operation_failure_demotes_active_to_error() {
        check_apply(
            Status::Active,
            StatusEvent::OperationFailed(config_error()),
            Some(Status::Error(config_error())),
        );
    }

    #[test]
    fn operation_failure_keeps_active() {
        check_apply(
            Status::Active,
            StatusEvent::OperationFailed(other_error()),
            Some(Status::Active),
        );
    }

    #[test]
    fn config_save_triggers_reinit_from_config_error_states() {
        for prior in [
            Status::Error(config_error()),
            Status::InitError(config_error()),
        ] {
            check_apply(prior, StatusEvent::ConfigSaved, Some(Status::Inactive));
        }
    }

    #[test]
    fn config_save_in_working_states_keeps_status() {
        for prior in [Status::Active, Status::Unauthenticated] {
            check_apply(prior.clone(), StatusEvent::ConfigSaved, Some(prior));
        }
    }

    #[test]
    fn invalid_transitions_are_rejected() {
        let cases = [
            (Status::Active, StatusEvent::InitStarted),
            (Status::Active, StatusEvent::LoggedIn),
            (Status::Initializing, StatusEvent::LoggedIn),
            (Status::Initializing, StatusEvent::LoggedOut),
            (Status::Inactive, StatusEvent::InitSucceeded),
            (Status::Inactive, StatusEvent::ConfigSaved),
            (Status::Initializing, StatusEvent::ConfigSaved),
            (
                Status::Unauthenticated,
                StatusEvent::OperationFailed(auth_error()),
            ),
        ];
        for (status, event) in cases {
            check_apply(status, event, None);
        }
    }
}
