#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceJob {
    pub source: String,
    pub priority: u32,
}

pub fn prioritize(mut jobs: Vec<SourceJob>) -> Vec<SourceJob> {
    jobs.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.source.cmp(&right.source))
    });
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_is_deterministic() {
        let result = prioritize(vec![
            SourceJob {
                source: "乙".into(),
                priority: 1,
            },
            SourceJob {
                source: "甲".into(),
                priority: 2,
            },
        ]);
        assert_eq!(result[0].source, "甲");
    }
}
