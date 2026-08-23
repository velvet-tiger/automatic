//! Skills that ship in the app binary rather than through the content
//! library.
//!
//! Two categories live here:
//!
//! - **Product/plugin-specific skills** authored by Automatic that describe
//!   how to use Automatic itself. Examples: the `automatic` skill (the MCP
//!   service surface) and `automatic-features` (the feature tracker, which
//!   will move to the features plugin when that plugin ships).
//! - **Third-party skills** redistributed with the app under their own
//!   licenses (see the per-skill `skill.json` metadata). Laravel, Pennant,
//!   PHP, Python, Tailwind CSS, Terraform, Vercel/React.
//!
//! Everything in `automatic-library` is Automatic-authored engineering
//! content and reaches the app through the `bundled_library` module; this
//! module holds the residue that does not fit that scope.

/// Skills bundled by the app, keyed by name. Each entry is
/// `(name, SKILL.md content)`.
pub const APP_SKILL_CONTENTS: &[(&str, &str)] = &[
    // -- Product/plugin-specific ---------------------------------------------
    (
        "automatic",
        include_str!("../../assets/skills/automatic/SKILL.md"),
    ),
    (
        "automatic-features",
        include_str!("../../assets/skills/automatic-features/SKILL.md"),
    ),
    // -- Third-party (template-only) -----------------------------------------
    (
        "vercel-react-best-practices",
        include_str!("../../assets/skills/vercel-react-best-practices/SKILL.md"),
    ),
    (
        "tailwindcss-development",
        include_str!("../../assets/skills/tailwindcss-development/SKILL.md"),
    ),
    (
        "laravel-specialist",
        include_str!("../../assets/skills/laravel-specialist/SKILL.md"),
    ),
    (
        "pennant-development",
        include_str!("../../assets/skills/pennant-development/SKILL.md"),
    ),
    (
        "terraform-skill",
        include_str!("../../assets/skills/terraform-skill/SKILL.md"),
    ),
    (
        "php-pro",
        include_str!("../../assets/skills/php-pro/SKILL.md"),
    ),
    (
        "python-pro",
        include_str!("../../assets/skills/python-pro/SKILL.md"),
    ),
];

/// Names of app-bundled skills that should be installed by default on first
/// run. The rest of `APP_SKILL_CONTENTS` is template-only (installed on
/// demand from a project template or from the discover UI).
pub const APP_AUTO_INSTALL: &[&str] = &["automatic", "automatic-features"];

/// Companion resource files for app-bundled skills. Same shape as the old
/// `BUNDLED_SKILL_RESOURCES`: `(skill_name, relative_path, content)`.
pub const APP_SKILL_RESOURCES: &[(&str, &str, &str)] = &[];

/// Look up a skill's SKILL.md content by name. Returns `None` if the name is
/// not in `APP_SKILL_CONTENTS`.
pub fn find(name: &str) -> Option<&'static str> {
    APP_SKILL_CONTENTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| *content)
}

/// All names shipped in `APP_SKILL_CONTENTS`.
pub fn names() -> Vec<&'static str> {
    APP_SKILL_CONTENTS.iter().map(|(n, _)| *n).collect()
}
