use std::collections::HashSet;
use semsimian::ClosureAndICMap;

extern crate semsimian;
use semsimian::similarity::calculate_semantic_jaccard_similarity;
use semsimian::utils::convert_list_of_tuples_to_hashmap;

#[test]
fn integration_test_semantic_jaccard_similarity() {
    let list_of_tuples = vec![
        ("apple".to_string(), "is_a".to_string(), "fruit".to_string()),
        (
            "apple".to_string(),
            "subclass_of".to_string(),
            "red".to_string(),
        ),
        (
            "cherry".to_string(),
            "subclass_of".to_string(),
            "red".to_string(),
        ),
        (
            "cherry".to_string(),
            "is_a".to_string(),
            "fruit".to_string(),
        ),
        (
            "cherry".to_string(),
            "is_a".to_string(),
            "seeded_fruit".to_string(),
        ),
        (
            "seeded_fruit".to_string(),
            "is_a".to_string(),
            "fruit".to_string(),
        ),
    ];
    let predicate_set = Some(HashSet::from(["is_a".to_string()]));
    // let (closure_table, _)
    let closure_ic_map: ClosureAndICMap = convert_list_of_tuples_to_hashmap(&list_of_tuples, &predicate_set);
    let sem_jaccard = calculate_semantic_jaccard_similarity(
        &closure_ic_map.closure_map,
        "apple",
        "cherry",
        &Some(HashSet::from(["is_a".to_string()])),
    );

    assert_eq!(sem_jaccard, 0.5)
}
