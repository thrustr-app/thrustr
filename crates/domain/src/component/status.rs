use crate::component::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Inactive,
    Initializing,
    Active,
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
        matches!(self, Self::Active)
            || matches!(
                self,
                Self::Error(e) | Self::InitError(e)
                if !matches!(e, Error::Auth(_))
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

    #[test]
    fn init_lifecycle() {
        let status = Status::Inactive.apply(StatusEvent::InitStarted).unwrap();
        assert!(status.is_initializing());

        let status = status.apply(StatusEvent::InitSucceeded).unwrap();
        assert!(status.is_active());

        let failed = Status::Initializing
            .apply(StatusEvent::InitFailed(config_error()))
            .unwrap();
        assert!(failed.is_init_error());
    }

    #[test]
    fn init_auth_failure_lands_init_error() {
        let status = Status::Initializing
            .apply(StatusEvent::InitFailed(auth_error()))
            .unwrap();
        assert!(status.is_init_error());
        assert!(status.can_login());
    }

    #[test]
    fn login_always_requires_reinit() {
        for prior in [
            Status::Unauthenticated,
            Status::Error(auth_error()),
            Status::InitError(auth_error()),
        ] {
            let status = prior.apply(StatusEvent::LoggedIn).unwrap();
            assert!(status.is_inactive());
            assert!(status.can_init());
        }
    }

    #[test]
    fn logout_lands_unauthenticated() {
        let status = Status::Active.apply(StatusEvent::LoggedOut).unwrap();
        assert!(matches!(status, Status::Unauthenticated));
    }

    #[test]
    fn auth_operation_failure_lands_error() {
        let status = Status::Active
            .apply(StatusEvent::OperationFailed(auth_error()))
            .unwrap();
        assert!(status.is_error());
        assert!(status.can_login());
    }

    #[test]
    fn config_operation_failure_is_recoverable_via_reconfigure() {
        let status = Status::Active
            .apply(StatusEvent::OperationFailed(config_error()))
            .unwrap();
        assert!(status.is_error());
        assert!(status.can_configure());
        assert!(!status.can_init());
    }

    #[test]
    fn config_save_triggers_reinit_from_config_error_states() {
        for prior in [
            Status::Error(config_error()),
            Status::InitError(config_error()),
        ] {
            let status = prior.apply(StatusEvent::ConfigSaved).unwrap();
            assert!(status.is_inactive());
            assert!(status.can_init());
        }
    }

    #[test]
    fn config_save_in_working_states_keeps_status() {
        for prior in [Status::Active, Status::Unauthenticated] {
            let status = prior.clone().apply(StatusEvent::ConfigSaved).unwrap();
            assert_eq!(status, prior);
        }
    }

    #[test]
    fn transient_operation_failure_keeps_active() {
        let status = Status::Active
            .apply(StatusEvent::OperationFailed(other_error()))
            .unwrap();
        assert!(status.is_active());
    }

    #[test]
    fn invalid_transitions_rejected() {
        assert!(Status::Active.apply(StatusEvent::InitStarted).is_none());
        assert!(Status::Active.apply(StatusEvent::LoggedIn).is_none());
        assert!(Status::Initializing.apply(StatusEvent::LoggedIn).is_none());
        assert!(Status::Initializing.apply(StatusEvent::LoggedOut).is_none());
        assert!(Status::Inactive.apply(StatusEvent::InitSucceeded).is_none());
        assert!(Status::Inactive.apply(StatusEvent::ConfigSaved).is_none());
        assert!(
            Status::Initializing
                .apply(StatusEvent::ConfigSaved)
                .is_none()
        );
        assert!(
            Status::Unauthenticated
                .apply(StatusEvent::OperationFailed(auth_error()))
                .is_none()
        );
    }
}
