//! Transfer scheduling decisions (E7-S1 / E14-S1): pure functions over the
//! job list, driven by a simulated clock so they're unit-testable without
//! tokio. The UI's ticker applies a [`TickPlan`] each second.

use std::collections::HashMap;

use mk_core::job::{Job, JobState};

/// One scheduling tick's decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct TickPlan {
    /// Multiply every running job's rate by this to respect the bandwidth cap.
    pub scale: f64,
    /// Waiting job ids to promote to Running, up to the parallel limit.
    pub promote: Vec<String>,
    /// Failed job ids (past their backoff) to move back to Waiting.
    pub retry: Vec<String>,
}

/// Decide a tick. `elapsed_since_failure` maps job id -> whole seconds since
/// it entered the Failed state (the simulated clock).
pub fn plan_tick(
    jobs: &[Job],
    parallel: usize,
    bandwidth_limit_bps: f64,
    elapsed_since_failure: &HashMap<String, u64>,
) -> TickPlan {
    let running_total: f64 = jobs
        .iter()
        .filter(|j| j.state == JobState::Running)
        .map(|j| j.rate_bytes_per_s)
        .sum();
    let scale = if bandwidth_limit_bps > 0.0
        && running_total > bandwidth_limit_bps
        && running_total > 0.0
    {
        bandwidth_limit_bps / running_total
    } else {
        1.0
    };

    let running = jobs.iter().filter(|j| j.state == JobState::Running).count();
    let slots = parallel.saturating_sub(running);
    let promote: Vec<String> = jobs
        .iter()
        .filter(|j| j.state == JobState::Waiting)
        .take(slots)
        .map(|j| j.id.clone())
        .collect();

    let retry: Vec<String> = jobs
        .iter()
        .filter(|j| {
            j.state == JobState::Failed
                && j.attempt < j.max_attempts
                && elapsed_since_failure
                    .get(&j.id)
                    .map(|elapsed| *elapsed >= backoff(j.attempt))
                    .unwrap_or(false)
        })
        .map(|j| j.id.clone())
        .collect();

    TickPlan {
        scale,
        promote,
        retry,
    }
}

/// Exponential backoff before the next auto-retry: `5 · 2^attempt` seconds.
pub fn backoff(attempt: u32) -> u64 {
    // Saturate so an extreme attempt counter can't overflow u64 (debug panic)
    // or wrap to 0 and busy-loop the retry.
    5u64.saturating_mul(2u64.saturating_pow(attempt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mk_core::job::{Direction, Job};

    fn job(id: &str, state: JobState, rate: f64, attempt: u32, max: u32) -> Job {
        Job {
            id: id.into(),
            direction: Direction::Up,
            name: id.into(),
            host_id: "h".into(),
            remote_path: "/r".into(),
            local_path: "/l".into(),
            bytes_done: 0,
            bytes_total: 100,
            rate_bytes_per_s: rate,
            eta_seconds: None,
            state,
            attempt,
            max_attempts: max,
            errno: None,
            message: None,
            finished_at: None,
            verified: None,
            ..Job::default()
        }
    }

    #[test]
    fn caps_aggregate_rate_at_the_limit() {
        let jobs = vec![
            job("a", JobState::Running, 15.0, 0, 3),
            job("b", JobState::Running, 10.0, 0, 3),
        ];
        let plan = plan_tick(&jobs, 3, 20.0, &HashMap::new());
        // total 25 > 20 -> scale to exactly 20
        assert!((plan.scale - 0.8).abs() < 1e-9);
    }

    #[test]
    fn under_the_cap_is_scale_one() {
        let jobs = vec![job("a", JobState::Running, 8.0, 0, 3)];
        let plan = plan_tick(&jobs, 3, 20.0, &HashMap::new());
        assert_eq!(plan.scale, 1.0);
    }

    #[test]
    fn respects_the_parallel_limit() {
        let jobs = vec![
            job("r1", JobState::Running, 1.0, 0, 3),
            job("r2", JobState::Running, 1.0, 0, 3),
            job("w1", JobState::Waiting, 0.0, 0, 3),
            job("w2", JobState::Waiting, 0.0, 0, 3),
            job("w3", JobState::Waiting, 0.0, 0, 3),
        ];
        let plan = plan_tick(&jobs, 3, 20.0, &HashMap::new());
        assert_eq!(plan.promote, vec!["w1"]); // 2 running, 1 slot
    }

    #[test]
    fn no_promotion_when_full() {
        let jobs = vec![
            job("r1", JobState::Running, 1.0, 0, 3),
            job("r2", JobState::Running, 1.0, 0, 3),
            job("r3", JobState::Running, 1.0, 0, 3),
            job("w1", JobState::Waiting, 0.0, 0, 3),
        ];
        let plan = plan_tick(&jobs, 3, 20.0, &HashMap::new());
        assert!(plan.promote.is_empty());
    }

    #[test]
    fn retries_failed_jobs_after_backoff_until_max_attempts() {
        let mut elapsed = HashMap::new();
        // attempt 2 -> backoff 20s. at 19s: no retry; at 20s: retry.
        elapsed.insert("f1".to_string(), 19);
        let jobs = vec![job("f1", JobState::Failed, 0.0, 2, 3)];
        assert!(plan_tick(&jobs, 3, 20.0, &elapsed).retry.is_empty());

        elapsed.insert("f1".to_string(), 20);
        assert_eq!(plan_tick(&jobs, 3, 20.0, &elapsed).retry, vec!["f1"]);

        // At max attempts it never auto-retries.
        let exhausted = vec![job("f1", JobState::Failed, 0.0, 3, 3)];
        assert!(plan_tick(&exhausted, 3, 20.0, &elapsed).retry.is_empty());
    }

    #[test]
    fn backoff_is_exponential() {
        assert_eq!(backoff(0), 5);
        assert_eq!(backoff(1), 10);
        assert_eq!(backoff(2), 20);
    }
}
