use crate::plugin::PluginState;
use crate::wit::thrustr::plugin::timers::{Error as TimersError, Host as TimersHost};
use domain::component::Schedule;
use std::time::Duration;

impl TimersHost for PluginState {
    fn schedule(
        &mut self,
        task: String,
        args: Vec<u8>,
        delay_ms: u64,
        interval_ms: Option<u64>,
    ) -> Result<(), TimersError> {
        let schedule = Schedule {
            delay: Duration::from_millis(delay_ms),
            interval: interval_ms.map(Duration::from_millis),
        };

        self.timers
            .schedule(task, args, schedule)
            .map_err(|e| TimersError::Other(e.to_string()))
    }

    fn cancel(&mut self, task: String) {
        self.timers.cancel(&task);
    }
}
