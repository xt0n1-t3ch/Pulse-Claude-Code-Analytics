#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudePlan {
    pub key: &'static str,
    pub name: &'static str,
    pub display_name: &'static str,
    pub badge_name: &'static str,
}

pub const CLAUDE_PLANS: &[ClaudePlan] = &[
    ClaudePlan {
        key: "free",
        name: "Free",
        display_name: "Free",
        badge_name: "FREE",
    },
    ClaudePlan {
        key: "pro",
        name: "Pro",
        display_name: "Pro ($20/mo)",
        badge_name: "PRO",
    },
    ClaudePlan {
        key: "max_5x",
        name: "Max 5x",
        display_name: "Max ($100/mo)",
        badge_name: "MAX 5x",
    },
    ClaudePlan {
        key: "max_20x",
        name: "Max 20x",
        display_name: "Max ($200/mo)",
        badge_name: "MAX 20x",
    },
    ClaudePlan {
        key: "max",
        name: "Max",
        display_name: "Max",
        badge_name: "MAX",
    },
    ClaudePlan {
        key: "team",
        name: "Team",
        display_name: "Team",
        badge_name: "TEAM",
    },
    ClaudePlan {
        key: "enterprise",
        name: "Enterprise",
        display_name: "Enterprise",
        badge_name: "ENTERPRISE",
    },
];

pub fn plan_from_key(key: &str) -> Option<&'static ClaudePlan> {
    CLAUDE_PLANS.iter().find(|plan| plan.key == key)
}

pub fn name_from_key(key: &str) -> String {
    plan_from_key(key)
        .map(|plan| plan.name.to_string())
        .unwrap_or_else(|| key.to_uppercase())
}

pub fn display_name_from_key(key: &str) -> Option<&'static str> {
    plan_from_key(key).map(|plan| plan.display_name)
}

pub fn badge_name_from_key(key: &str) -> Option<&'static str> {
    plan_from_key(key).map(|plan| plan.badge_name)
}

pub fn key_from_override(name: &str) -> Option<&'static str> {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "auto" {
        return None;
    }
    if normalized.contains("20x") {
        Some("max_20x")
    } else if normalized.contains("5x") {
        Some("max_5x")
    } else if normalized.contains("team") {
        Some("team")
    } else if normalized.contains("enterprise") {
        Some("enterprise")
    } else if normalized.contains("pro") {
        Some("pro")
    } else if normalized.contains("free") {
        Some("free")
    } else if normalized.contains("max") {
        Some("max")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_plan_keys_round_trip_through_names() {
        for plan in CLAUDE_PLANS {
            assert_eq!(name_from_key(plan.key), plan.name);
            assert_eq!(display_name_from_key(plan.key), Some(plan.display_name));
            assert_eq!(badge_name_from_key(plan.key), Some(plan.badge_name));
            assert_eq!(key_from_override(plan.name), Some(plan.key));
        }
    }

    #[test]
    fn claude_plan_override_parser_preserves_tolerance() {
        assert_eq!(key_from_override("Max 20x ($200/mo)"), Some("max_20x"));
        assert_eq!(key_from_override("Max 5x ($100/mo)"), Some("max_5x"));
        assert_eq!(key_from_override("  Team plan  "), Some("team"));
        assert_eq!(key_from_override("enterprise"), Some("enterprise"));
        assert_eq!(key_from_override("pro monthly"), Some("pro"));
        assert_eq!(key_from_override("free"), Some("free"));
        assert_eq!(key_from_override("Max"), Some("max"));
        assert_eq!(key_from_override("auto"), None);
        assert_eq!(key_from_override(""), None);
    }

    #[test]
    fn claude_plan_override_checks_max_multipliers_before_max() {
        assert_eq!(key_from_override("max 20x"), Some("max_20x"));
        assert_eq!(key_from_override("max 5x"), Some("max_5x"));
    }

    #[test]
    fn unknown_plan_name_falls_back_to_uppercase() {
        assert_eq!(name_from_key("custom"), "CUSTOM");
        assert_eq!(display_name_from_key("custom"), None);
        assert_eq!(badge_name_from_key("custom"), None);
        assert_eq!(key_from_override("custom"), None);
    }
}
