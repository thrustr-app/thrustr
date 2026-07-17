use crate::component::Error;

#[derive(Debug, Clone)]
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
    SyncFailed(Error),
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

    pub fn apply(&self, event: StatusEvent) -> Status {
        match (self, event) {
            (_, StatusEvent::InitStarted) => Self::Initializing,
            (_, StatusEvent::InitSucceeded) => Self::Active,
            (_, StatusEvent::InitFailed(e)) => Self::InitError(e),
            (Self::Unauthenticated, StatusEvent::LoggedIn) => Self::Active,
            (_, StatusEvent::LoggedIn) => Self::Inactive,
            (_, StatusEvent::LoggedOut) => Self::Unauthenticated,
            (_, StatusEvent::SyncFailed(e)) => Self::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_error() -> Error {
        Error::Auth("expired".into())
    }

    #[test]
    fn init_lifecycle() {
        let status = Status::Inactive.apply(StatusEvent::InitStarted);
        assert!(status.is_initializing());

        let status = status.apply(StatusEvent::InitSucceeded);
        assert!(status.is_active());

        let failed = Status::Initializing.apply(StatusEvent::InitFailed(auth_error()));
        assert!(failed.is_init_error());
    }

    #[test]
    fn login_from_unauthenticated_resumes_active() {
        let status = Status::Unauthenticated.apply(StatusEvent::LoggedIn);
        assert!(status.is_active());
    }

    #[test]
    fn login_from_error_states_requires_reinit() {
        for prior in [Status::Error(auth_error()), Status::InitError(auth_error())] {
            let status = prior.apply(StatusEvent::LoggedIn);
            assert!(status.is_inactive());
            assert!(status.can_init());
        }
    }

    #[test]
    fn logout_lands_unauthenticated() {
        let status = Status::Active.apply(StatusEvent::LoggedOut);
        assert!(matches!(status, Status::Unauthenticated));
    }

    #[test]
    fn sync_failure_lands_error() {
        let status = Status::Active.apply(StatusEvent::SyncFailed(auth_error()));
        assert!(status.is_error());
        assert!(status.can_login());
    }
}
