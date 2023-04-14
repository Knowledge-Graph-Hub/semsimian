use std::collections::{HashMap, HashSet};

use ordered_float::OrderedFloat;

pub fn calculate_jaccard_similarity(set1: &HashSet<i32>, set2: &HashSet<i32>) -> f64 {
    /* Returns Jaccard similarity between the two sets. */
    let intersection = set1.intersection(&set2).count();
    let union_measure = set1.union(&set2).count();
    let jaccard = intersection as f64 / union_measure as f64;
    jaccard
}

pub fn get_most_recent_common_ancestor_with_score(map: HashMap<String, f64>) -> (String, f64) {
    // Returns Inomration Content (IC) for entities.
    let (curie, max_ic) = map
        .into_iter()
        .max_by_key(|&(_, v)| OrderedFloat(v.abs()))
        .unwrap();
    (curie, max_ic)
}


pub fn calculate_phenomizer_score(map: HashMap<String, HashMap<String, f64>>,
                                  entity1: HashSet<String>,
                                  entity2: HashSet<String>) -> f64 {
    // calculate average resnik sim of all terms in entity1 and their best match in entity2
    let entity1_to_entity2_average_resnik_sim: f64 = pairwise_entity_score(&map, &entity1, &entity2);
    // now do the same for entity2 to entity1
    let entity2_to_entity1_average_resnik_sim: f64 = pairwise_entity_score(&map, &entity2, &entity1);
    // return the average of the two
    return (entity1_to_entity2_average_resnik_sim + entity2_to_entity1_average_resnik_sim)/2.0
}

fn pairwise_entity_resnik_score(map: &HashMap<String, HashMap<String, f64>>,
                                entity1: &HashSet<String>,
                                entity2: &HashSet<String>) -> f64{
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
    let entity1_to_entity2_average_resnik_sim = entity1_to_entity2_sum_resnik_sim / entity1.len() as f64;
    return entity1_to_entity2_average_resnik_sim;
}

#[cfg(test)]
mod tests {
    use crate::utils::numericize_sets;

    use super::*;

    #[test]
    fn test_calculate_jaccard_similarity() {
        let set1: HashSet<String> = HashSet::from([String::from("apple"), String::from("banana")]);
        let set2: HashSet<String> = HashSet::from([
            String::from("apple"),
            String::from("banana"),
            String::from("fruit"),
            String::from("tropical"),
        ]);
        let (num_set1, num_set2, _) = numericize_sets(&set1, &set2);
        let result = calculate_jaccard_similarity(&num_set1, &num_set2);
        println!("{result}");
        assert_eq!(result, 0.5);
    }

    #[test]
    fn test_get_most_recent_common_ancestor_with_score() {
        let map: HashMap<String, f64> = HashMap::from([
            (String::from("CARO:0000000"), 21.05),
            (String::from("BFO:0000002"), 0.7069),
            (String::from("BFO:0000003"), 14.89),
        ]);
        let expected_tuple: (String, f64) = (String::from("CARO:0000000"), 21.05);

        let result = get_most_recent_common_ancestor_with_score(map);
        assert_eq!(result, expected_tuple);
    }

    #[test]
    fn test_calculate_phenomizer_score() {
        let map: HashMap<String, HashMap<String, f64>> = HashMap::from([
            (String::from("CARO:0000000"), HashMap::from(
                [(String::from("CARO:0000000"), 5.0),
                    (String::from("BFO:0000002"), 4.0),
                    (String::from("BFO:0000003"), 3.0)])),
            (String::from("BFO:0000002"), HashMap::from(
                [(String::from("CARO:0000000"), 2.0),
                    (String::from("BFO:0000002"), 4.0),
                    (String::from("BFO:0000003"), 3.0)])),
            (String::from("BFO:0000003"), HashMap::from(
                [(String::from("CARO:0000000"), 1.0),
                    (String::from("BFO:0000002"), 3.0),
                    (String::from("BFO:0000003"), 4.0)])),
        ]);

        let mut entity_one = HashSet::new();
        entity_one.insert(String::from("CARO:0000000")); // resnik of best match = 5
        entity_one.insert(String::from("BFO:0000002")); // resnik of best match = 4

        let mut entity_two = HashSet::new();
        entity_two.insert(String::from("BFO:0000003")); // resnik of best match = 3
        entity_two.insert(String::from("BFO:0000002")); // resnik of best match = 4
        entity_two.insert(String::from("CARO:0000000")); // resnik of best match = 5

        let expected = ((5.0 + 4.0) / 2.0 + (3.0 + 4.0 + 5.0) / 3.0) / 2.0;

        let result = calculate_phenomizer_score(map, entity_one, entity_two);
        assert_eq!(result, expected);
    }

}
