use crate::{utils::expand_term_using_closure, utils::predicate_set_to_key};
use ordered_float::OrderedFloat;
use std::collections::{HashMap, HashSet};

// type Predicate = String;
// type TermID = String;
// type PredicateSetKey = String;

pub fn calculate_semantic_jaccard_similarity(
    closure_table: &HashMap<&str, HashMap<&str, HashSet<&str>>>,
    entity1: &str,
    entity2: &str,
    predicates: &Option<HashSet<&str>>,
) -> f64 {
    /* Returns semantic Jaccard similarity between the two sets. */
    let entity1_closure = expand_term_using_closure(&entity1, closure_table, &predicates);
    let entity2_closure = expand_term_using_closure(&entity2, closure_table, &predicates);
    let jaccard = calculate_jaccard_similarity_str(&entity1_closure, &entity2_closure);
    jaccard
}

pub fn calculate_jaccard_similarity_str(set1: &HashSet<&str>, set2: &HashSet<&str>) -> f64 {
    /* Returns Jaccard similarity between the two sets. */
    let intersection = set1.intersection(&set2).count();
    let union_measure = set1.union(&set2).count();
    let jaccard = intersection as f64 / union_measure as f64;
    jaccard
}

pub fn calculate_jaccard_similarity(set1: &HashSet<i32>, set2: &HashSet<i32>) -> f64 {
    /* Returns Jaccard similarity between the two sets. */
    let intersection = set1.intersection(&set2).count();
    let union_measure = set1.union(&set2).count();
    let jaccard = intersection as f64 / union_measure as f64;
    jaccard
}

pub fn get_most_recent_common_ancestor_with_score(map: HashMap<&str, f64>) -> (&str, f64) {
    // Returns Inomration Content (IC) for entities.
    let (curie, max_ic) = map
        .into_iter()
        .max_by_key(|&(_, v)| OrderedFloat(v.abs()))
        .unwrap();
    (curie, max_ic)
}

pub fn calculate_phenomizer_score(
    map: HashMap<&str, HashMap<&str, f64>>,
    entity1: HashSet<&str>,
    entity2: HashSet<&str>,
) -> f64 {
    // calculate average resnik sim of all terms in entity1 and their best match in entity2
    let entity1_to_entity2_average_resnik_sim: f64 =
        pairwise_entity_resnik_score(&map, &entity1, &entity2);
    // now do the same for entity2 to entity1
    let entity2_to_entity1_average_resnik_sim: f64 =
        pairwise_entity_resnik_score(&map, &entity2, &entity1);
    // return the average of the two
    return (entity1_to_entity2_average_resnik_sim + entity2_to_entity1_average_resnik_sim) / 2.0;
}

pub fn pairwise_entity_resnik_score(
    map: &HashMap<&str, HashMap<&str, f64>>,
    entity1: &HashSet<&str>,
    entity2: &HashSet<&str>,
) -> f64 {
    let mut entity1_to_entity2_sum_resnik_sim = 0.0;

    for e1_term in entity1.clone().into_iter() {
        let mut max_resnik_sim_e1_e2 = 0.0;
        for e2_term in entity2.clone().into_iter() {
            // NB: this will definitely fail if the term is not in the map
            let mica = map.get(&e1_term).unwrap().get(&e2_term).unwrap();
            if mica > &max_resnik_sim_e1_e2 {
                max_resnik_sim_e1_e2 = *mica;
            }
        }
        entity1_to_entity2_sum_resnik_sim += max_resnik_sim_e1_e2;
    }
    let entity1_to_entity2_average_resnik_sim =
        entity1_to_entity2_sum_resnik_sim / entity1.len() as f64;
    return entity1_to_entity2_average_resnik_sim;
}

pub fn calculate_max_information_content(
    closure_map: &HashMap<&str, HashMap<&str, HashSet<&str>>>,
    ic_map: &HashMap<&str, HashMap<&str, f64>>,
    entity1: &str,
    entity2: &str,
    predicates: &Option<HashSet<&str>>,
) -> f64 {
    // CODE TO CALCULATE MAX IC
    let filtered_common_ancestors: Vec<&str> =
        common_ancestors(closure_map, entity1, entity2, predicates);

    let predicate_set_key = predicate_set_to_key(predicates);

    // for each member of filtered_common_ancestors, find the entry for it in ic_map
    let mut max_ic: f64 = 0.0;
    for ancestor in filtered_common_ancestors.iter() {
        if let Some(ic) = ic_map
            .get(&*predicate_set_key)
            .expect("Finding ancestor in ic map")
            .get(ancestor)
        {
            if *ic > max_ic {
                max_ic = *ic;
            }
        }
    }
    // then return the String and f64 for the filtered_common_ancestors with the highest f64
    max_ic
}

/// Returns the common ancestors of two entities based on the given closure table and a set of predicates.

fn common_ancestors<'a>(
    closure_map: &'a HashMap<&'a str, HashMap<&'a str, HashSet<&'a str>>>,

    // {"GO:1234": {'is_a': {'GO:5678', 'GO:9012'}, 'part_of': {'GO:3456', 'GO:7890'}}}

    // {"GO:5678": ('is_a', 'part_of'): {['GO:3456', 'GO:7890']}}\

    // {"GO:5678": vec![('is_a', 'part_of')]: {['GO:3456', 'GO:7890']}}

    // {"GO:5678": 'is_a_+_part_of': {['GO:3456', 'GO:7890']}}
    entity1: &'a str,
    entity2: &'a str,
    predicates: &'a Option<HashSet<&'a str>>,
) -> Vec<&'a str> {
    // expand_term_using_closure() handles case of the entity being not present -> returning empty set
    let entity1_closure = expand_term_using_closure(entity1, closure_map, predicates);
    let entity2_closure = expand_term_using_closure(entity2, closure_map, predicates);

    entity1_closure
        .into_iter()
        .filter(|ancestor| entity2_closure.contains(ancestor))
        .collect()
}

fn _filter_ancestors_by_predicates(
    // currently this is not used, but it might be useful in the future
    ancestors: &HashMap<String, HashSet<String>>,
    predicates: &Option<HashSet<String>>,
) -> HashSet<String> {
    match predicates {
        Some(preds) => {
            let mut filtered = HashSet::new();
            for (predicate, entities) in ancestors {
                if preds.contains(predicate) {
                    for entity in entities {
                        filtered.insert(entity.clone());
                    }
                }
            }
            filtered
        }
        None => ancestors
            .values()
            .flat_map(|entities| entities.iter().cloned())
            .collect(),
    }
}

// scores: maps ancestors to corresponding IC scores
fn _mrca_and_score<'a>(scores: &'a HashMap<&'a str, f64>) -> (Option<&'a str>, f64) {
    let mut max_ic = 0.0;
    let mut mrca = None;

    for (ancestor, ic) in scores.iter() {
        if *ic > max_ic {
            max_ic = *ic;
            mrca = Some(ancestor.clone());
        }
    }
    (mrca, max_ic)
}

// TODO: provide a way to specify 'bespoke' information contents for each term
// for example, in a population of patients of interest
fn _calculate_information_content_scores<'a>(
    filtered_common_ancestors: &'a Vec<&'a str>,
    closure_table: &'a HashMap<&'a str, HashMap<&'a str, HashSet<&'a str>>>,
    predicates: &'a Option<HashSet<&'a str>>,
) -> HashMap<&'a str, f64> {
    let (term_frequencies, corpus_size) =
        calculate_term_frequencies_and_corpus_size(closure_table, predicates);

    let mut ic_scores = HashMap::new();
    for ancestor in filtered_common_ancestors {
        if let Some(freq) = term_frequencies.get(ancestor) {
            let probability = *freq as f64 / corpus_size as f64;
            let ic = -probability.log2();
            ic_scores.insert(ancestor.clone(), ic);
        }
    }
    ic_scores
}

fn calculate_term_frequencies_and_corpus_size<'a>(
    closure_table: &'a HashMap<&'a str, HashMap<&'a str, HashSet<&'a str>>>,
    predicates: &Option<HashSet<&'a str>>,
) -> (HashMap<&'a str, usize>, usize) {
    let mut term_frequencies = HashMap::new();
    let mut corpus_size = 0;

    for (_entity, predicate_map) in closure_table {
        for (predicate, terms) in predicate_map {
            if predicates.is_none() || predicates.as_ref().unwrap().contains(predicate) {
                for term in terms {
                    *term_frequencies.entry(term.clone()).or_insert(0) += 1;
                }
                corpus_size += terms.len();
            }
        }
    }

    (term_frequencies, corpus_size)
}

#[cfg(test)]
mod tests {
    use crate::utils::numericize_sets;

    use super::*;

    #[test]
    fn test_semantic_jaccard_similarity() {
        let mut closure_map: HashMap<&str, HashMap<&str, HashSet<&str>>> = HashMap::new();

        // closure map looks like this:
        // +subClassOf -> CARO:0000000 -> CARO:0000000, BFO:0000002, BFO:0000003
        //             -> BFO:0000002 -> BFO:0000002, BFO:0000003
        //             -> BFO:0000003 -> BFO:0000003
        //             -> BFO:0000004 -> BFO:0000004

        let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut set: HashSet<&str> = HashSet::new();
        set.insert("CARO:0000000");
        set.insert("BFO:0000002");
        set.insert("BFO:0000003");
        map.insert("CARO:0000000", set);

        let mut set: HashSet<&str> = HashSet::new();
        set.insert("BFO:0000002");
        set.insert("BFO:0000003");
        map.insert("BFO:0000002", set);

        let mut set: HashSet<&str> = HashSet::new();
        set.insert("BFO:0000003");
        map.insert("BFO:0000003", set);
        closure_map.insert("+subClassOf", map);

        // make another closure map for subclassof + partof
        // +partOf+subClassOf -> CARO:0000000 -> CARO:0000000, BFO:0000002, BFO:0000003
        //             -> BFO:0000002 -> BFO:0000002, BFO:0000003
        //             -> BFO:0000003 -> BFO:0000003, BFO:0000004 <- +partOf
        //             -> BFO:0000004 -> BFO:0000004
        let mut closure_map2: HashMap<&str, HashMap<&str, HashSet<&str>>> = HashMap::new();
        closure_map2.insert(
            "+partOf+subClassOf",
            closure_map.get("+subClassOf").unwrap().clone(),
        );
        closure_map2
            .get_mut("+partOf+subClassOf")
            .unwrap()
            .get_mut("BFO:0000003")
            .unwrap()
            .insert("BFO:0000004");

        let mut sco_predicate: HashSet<&str> = HashSet::new();
        sco_predicate.insert("subClassOf");

        let result = calculate_semantic_jaccard_similarity(
            &closure_map,
            "CARO:0000000",
            "BFO:0000002",
            &Some(sco_predicate.clone()),
        );
        println!("{result}");
        assert_eq!(result, 2.0 / 3.0);

        let result2 = calculate_semantic_jaccard_similarity(
            &closure_map,
            "BFO:0000002",
            "BFO:0000003",
            &Some(sco_predicate.clone()),
        );
        println!("{result2}");
        assert_eq!(result2, 0.5);

        let mut sco_po_predicate: HashSet<&str> = HashSet::new();
        sco_po_predicate.insert("subClassOf");
        sco_po_predicate.insert("partOf");

        // with the refactor of closure map, this test doesn't really test anything more than
        // the previous tests
        let result3 = calculate_semantic_jaccard_similarity(
            &closure_map2,
            "BFO:0000002",
            "BFO:0000003",
            &Some(sco_po_predicate),
        );
        println!("{result3}");
        assert_eq!(result3, 1.0 / 3.0);
    }

    #[test]
    fn test_calculate_jaccard_similarity() {
        let set1: HashSet<&str> = HashSet::from(["apple", "banana"]);
        let set2: HashSet<&str> = HashSet::from(["apple", "banana", "fruit", "tropical"]);
        let (num_set1, num_set2, _) = numericize_sets(&set1, &set2);
        let result = calculate_jaccard_similarity(&num_set1, &num_set2);
        println!("{result}");
        assert_eq!(result, 0.5);
    }

    #[test]
    fn test_calculate_jaccard_similarity_str() {
        let set1: HashSet<&str> = HashSet::from(["apple", "banana"]);
        let set2: HashSet<&str> = HashSet::from(["apple", "banana", "fruit", "tropical"]);
        let result = calculate_jaccard_similarity_str(&set1, &set2);
        println!("{result}");
        assert_eq!(result, 0.5);
    }

    #[test]
    fn test_get_most_recent_common_ancestor_with_score() {
        let map: HashMap<&str, f64> = HashMap::from([
            ("CARO:0000000", 21.05),
            ("BFO:0000002", 0.7069),
            ("BFO:0000003", 14.89),
        ]);
        let expected_tuple: (&str, f64) = ("CARO:0000000", 21.05);

        let result = get_most_recent_common_ancestor_with_score(map);
        assert_eq!(result, expected_tuple);
    }

    #[test]
    fn test_calculate_phenomizer_score() {
        let map: HashMap<&str, HashMap<&str, f64>> = HashMap::from([
            (
                "CARO:0000000",
                HashMap::from([
                    ("CARO:0000000", 5.0),
                    ("BFO:0000002", 4.0),
                    ("BFO:0000003", 3.0),
                ]),
            ),
            (
                "BFO:0000002",
                HashMap::from([
                    ("CARO:0000000", 2.0),
                    ("BFO:0000002", 4.0),
                    ("BFO:0000003", 3.0),
                ]),
            ),
            (
                "BFO:0000003",
                HashMap::from([
                    ("CARO:0000000", 1.0),
                    ("BFO:0000002", 3.0),
                    ("BFO:0000003", 4.0),
                ]),
            ),
        ]);

        let mut entity_one = HashSet::new();
        entity_one.insert("CARO:0000000"); // resnik of best match = 5
        entity_one.insert("BFO:0000002"); // resnik of best match = 4

        let mut entity_two = HashSet::new();
        entity_two.insert("BFO:0000003"); // resnik of best match = 3
        entity_two.insert("BFO:0000002"); // resnik of best match = 4
        entity_two.insert("CARO:0000000"); // resnik of best match = 5

        let expected = ((5.0 + 4.0) / 2.0 + (3.0 + 4.0 + 5.0) / 3.0) / 2.0;

        let result = calculate_phenomizer_score(map, entity_one, entity_two);
        assert_eq!(result, expected);
    }

    // TODO: test that closure map in Semsimian object is correct
    // TODO: test that ic map in Semsimian object is correct

    #[test]
    fn test_calculate_max_information_content() {
        let ic_map: HashMap<&str, HashMap<&str, f64>> = [(
            "+subClassOf",
            [
                ("CARO:0000000", 2.585),
                ("BFO:0000002", 1.585),
                ("BFO:0000003", 1.0),
            ]
            .iter()
            .cloned()
            .collect(),
        )]
        .iter()
        .cloned()
        .collect();

        // closure map looks like this:
        // {'subClassOf': {'CARO:0000000': {'CARO:0000000', 'BFO:0000002', 'BFO:0000003'},
        //                 'BFO:0000002':  {'BFO:0000002', 'BFO:0000003'},
        //                 'BFO:0000003':  {'BFO:0000003'}}}

        let mut closure_map: HashMap<&str, HashMap<&str, HashSet<&str>>> = HashMap::new();

        let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut set: HashSet<&str> = HashSet::new();
        set.insert("CARO:0000000");
        set.insert("BFO:0000002");
        set.insert("BFO:0000003");
        map.insert("CARO:0000000", set.clone());
        closure_map.insert("+subClassOf", map.clone());

        set.clear();
        set.insert("BFO:0000002");
        set.insert("BFO:0000003");
        map.insert("BFO:0000002", set.clone());
        closure_map.insert("+subClassOf", map.clone());

        set.clear();
        set.insert("BFO:0000003");
        map.insert("BFO:0000003", set.clone());
        closure_map.insert("+subClassOf", map);

        // Term frequencies:
        // "CARO:0000000": 1
        // "BFO:0000002": 2
        // "BFO:0000003": 3
        //
        // Corpus size: 6 (sum of term frequencies)
        //
        // Information Content (IC) scores:
        // IC("CARO:0000000") = -log2(1/6) ≈ 2.585
        // IC("BFO:0000002") = -log2(2/6) ≈ 1.585
        // IC("BFO:0000003") = -log2(3/6) ≈ 1
        //
        // Max IC for "CARO:0000000" and "BFO:0000002":
        // Common ancestors: "BFO:0000002" and "BFO:0000003"
        // Max IC: 1.585 (IC of "BFO:0000002")

        let predicates = Some(HashSet::from(["subClassOf"]));
        let result = calculate_max_information_content(
            &closure_map,
            &ic_map,
            "CARO:0000000",
            "BFO:0000002",
            &predicates,
        );
        println!("Max IC: {}", result);
        let expected_value = 1.585;
        assert!(
            (result - expected_value).abs() < 1e-3,
            "Expected value: {}, got: {}",
            expected_value,
            result
        );
    }
}
