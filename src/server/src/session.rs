use chrono::NaiveDateTime;
use random_word::Lang;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub fn generate_session_id() -> String {
    let w1 = random_word::get(Lang::En);
    let w2 = random_word::get(Lang::En);
    let w3 = random_word::get(Lang::En);
    format!("{w1}-{w2}-{w3}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_session_id_shape() {
        let id = generate_session_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 3, "id {id:?} should have 3 segments");
        assert!(parts.iter().all(|p| !p.is_empty()));
    }
}
