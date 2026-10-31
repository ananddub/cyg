
#[cfg(test)]
mod tests {
    use super::super::*;

    fn props() -> (HashMap<String, String>, HashMap<String, f64>) {
        (
            HashMap::from([("mode".to_string(), "deathmatch".to_string())]),
            HashMap::from([("rank".to_string(), 42.0f64)]),
        )
    }

    #[test]
    fn empty_query_matches_anything() {
        let (s, n) = props();
        let svc = RealtimeMatchmakerService::new();
        assert!(svc.query_matches("", &s, &n));
    }

    #[test]
    fn equality_and_comparisons() {
        let (s, n) = props();
        let svc = RealtimeMatchmakerService::new();
        assert!(svc.query_matches("+properties.mode:deathmatch", &s, &n));
        assert!(!svc.query_matches("+properties.mode:capture", &s, &n));
        assert!(svc.query_matches("+properties.rank:>=40", &s, &n));
        assert!(!svc.query_matches("+properties.rank:>42", &s, &n));
        assert!(svc.query_matches("-properties.mode:capture", &s, &n));
        assert!(!svc.query_matches("-properties.mode:deathmatch", &s, &n));
    }

    #[test]
    fn mode_bucketing_and_rank_indexing() {
        let svc = RealtimeMatchmakerService::new();
        let q1 = "+properties.mode:deathmatch".to_string();
        let t1 = Ticket {
            id: Uuid::new_v4(),
            sid: Uuid::new_v4(),
            uid: Uuid::new_v4(),
            username: "player1".into(),
            min_count: 2,
            max_count: 4,
            compiled_terms: svc.compile_query(&q1),
            query: q1,
            string_props: HashMap::from([("mode".into(), "deathmatch".into())]),
            numeric_props: HashMap::from([("rank".into(), 25.0)]),
            party_id: None,
            created_at: Utc::now(),
        };
        let q2 = "+properties.mode:battle_royale".to_string();
        let t2 = Ticket {
            id: Uuid::new_v4(),
            sid: Uuid::new_v4(),
            uid: Uuid::new_v4(),
            username: "player2".into(),
            min_count: 2,
            max_count: 4,
            compiled_terms: svc.compile_query(&q2),
            query: q2,
            string_props: HashMap::from([("mode".into(), "battle_royale".into())]),
            numeric_props: HashMap::from([("rank".into(), 85.0)]),
            party_id: None,
            created_at: Utc::now(),
        };

        assert_eq!(svc.extract_mode(&t1), "deathmatch");
        assert_eq!(svc.extract_mode(&t2), "battle_royale");

        svc.index_ticket(&t1);
        svc.index_ticket(&t2);

        assert!(
            svc.mode_index
                .get_sync("deathmatch")
                .unwrap()
                .contains_sync(&t1.id)
        );
        assert!(
            svc.mode_index
                .get_sync("battle_royale")
                .unwrap()
                .contains_sync(&t2.id)
        );

        let low_rank = svc.find_tickets_in_rank_range(20, 30);
        assert_eq!(low_rank.len(), 1);
        assert_eq!(low_rank[0], t1.id);

        let high_rank = svc.find_tickets_in_rank_range(80, 90);
        assert_eq!(high_rank.len(), 1);
        assert_eq!(high_rank[0], t2.id);

        let none_rank = svc.find_tickets_in_rank_range(50, 60);
        assert!(none_rank.is_empty());

        svc.unindex_ticket(&t1);
        assert!(
            !svc.mode_index
                .get_sync("deathmatch")
                .unwrap()
                .contains_sync(&t1.id)
        );
        assert!(svc.find_tickets_in_rank_range(20, 30).is_empty());
    }

    #[test]
    fn precompiled_ast_zero_alloc_matching() {
        let svc = RealtimeMatchmakerService::new();
        let query = "+properties.mode:ranked +properties.rank:>=50 +properties.rank:<=100 -properties.banned:true";
        let terms = svc.compile_query(query);
        assert_eq!(terms.len(), 4);

        let valid_strings = HashMap::from([("mode".into(), "ranked".into())]);
        let valid_numerics = HashMap::from([("rank".into(), 75.0)]);
        assert!(svc.is_compatible(&terms, &valid_strings, &valid_numerics));

        let low_numerics = HashMap::from([("rank".into(), 40.0)]);
        assert!(!svc.is_compatible(&terms, &valid_strings, &low_numerics));

        let high_numerics = HashMap::from([("rank".into(), 120.0)]);
        assert!(!svc.is_compatible(&terms, &valid_strings, &high_numerics));

        let banned_strings = HashMap::from([
            ("mode".into(), "ranked".into()),
            ("banned".into(), "true".into()),
        ]);
        assert!(!svc.is_compatible(&terms, &banned_strings, &valid_numerics));
    }
}
