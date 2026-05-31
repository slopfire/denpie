use rand::Rng;

pub const DEFAULT_TOPIC_ICON: &str = "lucide:tag";

pub fn hash_string(value: &str) -> u64 {
    value.bytes().fold(5381u64, |hash, byte| {
        hash.wrapping_mul(33).wrapping_add(u64::from(byte))
    })
}

pub fn color_hue_from_name(name: &str) -> i32 {
    (hash_string(name) % 360) as i32
}

pub fn color_from_hue(hue: i32) -> String {
    let hue = hue.rem_euclid(360);
    format!("hsl({hue}, 65%, 55%)")
}

pub fn topic_color(name: &str) -> String {
    color_from_hue(resolve_color_hue(None, name))
}

pub fn resolve_color_hue(stored: Option<i32>, name: &str) -> i32 {
    stored
        .filter(|hue| (0..360).contains(hue))
        .unwrap_or_else(|| color_hue_from_name(name))
}

pub fn resolve_topic_color(stored: Option<i32>, name: &str) -> String {
    match stored.filter(|hue| (0..360).contains(hue)) {
        Some(hue) => color_from_hue(hue),
        None => topic_color(name),
    }
}

pub fn random_color_hue() -> i32 {
    rand::thread_rng().gen_range(0..360)
}

pub fn random_color_hue_excluding(current: Option<i32>) -> i32 {
    let mut hue = random_color_hue();
    if let Some(current) = current.filter(|value| (0..360).contains(value)) {
        if hue == current {
            hue = (hue + 137) % 360;
        }
    }
    hue
}

#[cfg(test)]
mod tests {
    use super::{
        color_from_hue, color_hue_from_name, hash_string, random_color_hue_excluding, topic_color,
    };

    #[test]
    fn topic_color_is_deterministic() {
        assert_eq!(topic_color("rust"), topic_color("rust"));
        assert!(topic_color("rust").starts_with("hsl("));
    }

    #[test]
    fn hash_string_differs_for_different_inputs() {
        assert_ne!(hash_string("alpha"), hash_string("beta"));
    }

    #[test]
    fn color_from_hue_wraps() {
        assert_eq!(color_from_hue(370), color_from_hue(10));
    }

    #[test]
    fn random_color_hue_excluding_changes_current() {
        let current = color_hue_from_name("rust");
        assert_ne!(random_color_hue_excluding(Some(current)), current);
    }
}
