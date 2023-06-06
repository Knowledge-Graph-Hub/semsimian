use pyo3::prelude::*;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
pub mod similarity;

pub mod utils;
use rayon::prelude::*;

mod test_utils;

use std::fmt;

use similarity::{calculate_max_information_content, calculate_phenomizer_score};
use utils::{convert_list_of_tuples_to_hashmap, expand_term_using_closure, predicate_set_to_key};

// change to "pub" because it is easier for testing
pub type Predicate = String;
pub type TermID = String;
pub type PredicateSetKey = String;
pub type Jaccard = f64;
pub type Resnik = f64;
pub type Phenodigm = f64;
pub type MostInformativeAncestors = HashSet<TermID>;

pub struct ClosureAndICMap {
    pub closure_map: HashMap<PredicateSetKey, HashMap<TermID, HashSet<TermID>>>,
    pub ic_map: HashMap<PredicateSetKey, HashMap<TermID, f64>>,
}

#[derive(Clone)]
pub struct RustSemsimian {
    spo: Vec<(TermID, Predicate, TermID)>,

    ic_map: HashMap<PredicateSetKey, HashMap<TermID, f64>>,
    // ic_map is something like {('is_a_+_part_of'), {'GO:1234': 1.234}}
    closure_map: HashMap<PredicateSetKey, HashMap<TermID, HashSet<TermID>>>,
    // closure_map is something like {('is_a_+_part_of'), {'GO:1234': {'GO:1234', 'GO:5678'}}}
}

impl RustSemsimian {
    // TODO: this is tied directly to Oak, and should be made more generic
    // TODO: also, we should support loading 'custom' ic
    // TODO: generate ic map and closure map using (spo).
    pub fn new(spo: Vec<(TermID, Predicate, TermID)>) -> RustSemsimian {
        RustSemsimian {
            spo,
            ic_map: HashMap::new(),
            closure_map: HashMap::new(),
        }
    }

    pub fn jaccard_similarity(
        &mut self,
        term1: &str,
        term2: &str,
        predicates: &Option<HashSet<Predicate>>,
    ) -> f64 {
        let closure_and_ic_map = self.get_closure_and_ic_map(predicates);

        let apple_set = expand_term_using_closure(term1, &closure_and_ic_map.closure_map, predicates);
        let fruit_set = expand_term_using_closure(term2, &closure_and_ic_map.closure_map, predicates);

        let intersection = apple_set.intersection(&fruit_set).count() as f64;
        let union = apple_set.union(&fruit_set).count() as f64;
        intersection / union
    }

    pub fn resnik_similarity(
        &mut self,
        term1: &str,
        term2: &str,
        predicates: &Option<HashSet<Predicate>>,
    ) -> (HashSet<String>, f64) {
        let closure_ic_map: ClosureAndICMap = self.get_closure_and_ic_map(predicates);
        calculate_max_information_content(&closure_ic_map.closure_map, &closure_ic_map.ic_map, term1, term2, predicates)
    }

    pub fn all_by_all_pairwise_similarity(
        &self,
        subject_terms: &HashSet<TermID>,
        object_terms: &HashSet<TermID>,
        predicates: &Option<HashSet<Predicate>>,
    ) -> HashMap<TermID, HashMap<TermID, (Jaccard, Resnik, Phenodigm, MostInformativeAncestors)>>
    {
        let self_shared = Arc::new(Mutex::new(self.clone()));

        let similarity_map: HashMap<
            TermID,
            HashMap<TermID, (Jaccard, Resnik, Phenodigm, MostInformativeAncestors)>,
        > = subject_terms
            .par_iter() // parallelize computations
            .map(|subject| {
                let mut subject_similarities: HashMap<
                    TermID,
                    (Jaccard, Resnik, Phenodigm, MostInformativeAncestors),
                > = HashMap::new();
                for object in object_terms.iter() {
                    let mut self_locked = self_shared.lock().unwrap();
                    let jaccard_sim = self_locked.jaccard_similarity(subject, object, predicates);
                    let (mica_ancestor_set, resnik_sim) =
                        self_locked.resnik_similarity(subject, object, predicates);
                    subject_similarities.insert(
                        object.clone(),
                        (
                            resnik_sim,
                            jaccard_sim,
                            (resnik_sim * jaccard_sim).sqrt(),
                            mica_ancestor_set,
                        ),
                    );
                }
                (subject.clone(), subject_similarities)
            })
            .collect();

        similarity_map
    }

    // TODO: make this predicate aware, and make it work with the new closure map
    pub fn phenomizer_score(
        map: HashMap<String, HashMap<String, f64>>,
        entity1: HashSet<String>,
        entity2: HashSet<String>,
    ) -> PyResult<f64> {
        Ok(calculate_phenomizer_score(map, entity1, entity2))
    }

    // get closure and ic map for a given set of predicates. if the closure and ic map for the given predicates doesn't exist, create them

    fn get_closure_and_ic_map(
        &mut self,
        predicates: &Option<HashSet<Predicate>>,
    ) -> ClosureAndICMap {
        let predicate_set_key = predicate_set_to_key(predicates);
        if !self.closure_map.contains_key(&predicate_set_key)
            || !self.ic_map.contains_key(&predicate_set_key)
        {
            let closure_and_ic_map: ClosureAndICMap = convert_list_of_tuples_to_hashmap(&self.spo, predicates);

            self.closure_map.insert(
                predicate_set_key.clone(),
                closure_and_ic_map.closure_map.get(&predicate_set_key).unwrap().clone(),
            );
            self.ic_map.insert(
                predicate_set_key.clone(),
                closure_and_ic_map.ic_map.get(&predicate_set_key).unwrap().clone(),
            );
        }

        ClosureAndICMap {
            closure_map: self.closure_map.clone(),
            ic_map: self.ic_map.clone(),
        }
    }

}

#[pyclass]
pub struct Semsimian {
    ss: RustSemsimian,
}

#[pymethods]
impl Semsimian {
    #[new]
    fn new(spo: Vec<(TermID, Predicate, TermID)>) -> PyResult<Self> {
        let ss = RustSemsimian::new(spo);
        Ok(Semsimian { ss })
    }

    fn jaccard_similarity(
        &mut self,
        term1: TermID,
        term2: TermID,
        predicates: Option<HashSet<Predicate>>,
    ) -> PyResult<f64> {
        Ok(self.ss.jaccard_similarity(&term1, &term2, &predicates))
    }

    fn resnik_similarity(
        &mut self,
        term1: TermID,
        term2: TermID,
        predicates: Option<HashSet<Predicate>>,
    ) -> PyResult<(HashSet<String>, f64)> {
        Ok(self.ss.resnik_similarity(&term1, &term2, &predicates))
    }

    fn all_by_all_pairwise_similarity(
        &mut self,
        subject_terms: HashSet<TermID>,
        object_terms: HashSet<TermID>,
        predicates: Option<HashSet<Predicate>>,
    ) -> HashMap<TermID, HashMap<TermID, (f64, f64, f64, HashSet<String>)>> {
        self.ss
            .all_by_all_pairwise_similarity(&subject_terms, &object_terms, &predicates)
    }
}

impl fmt::Debug for RustSemsimian {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RustSemsimian {{ spo: {:?}, ic_map: {:?}, closure_map: {:?} }}",
            self.spo, self.ic_map, self.closure_map
        )
    }
}

#[pymodule]
fn semsimian(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Semsimian>()?;
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::RustSemsimian;
    use std::collections::HashSet;

    #[test]
    fn test_jaccard_similarity() {
        let spo_cloned = crate::test_utils::test_constants::SPO_FRUITS.clone();
        let predicates: Option<HashSet<Predicate>> = Some(
            vec!["related_to"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
        );
        let no_predicates: Option<HashSet<Predicate>> = None;
        let mut ss = RustSemsimian::new(spo_cloned);
        let term1 = "apple".to_string();
        let term2 = "banana".to_string();
        let sim = ss.jaccard_similarity(&term1, &term2, &predicates);
        let sim2 = ss.jaccard_similarity(&term1, &term2, &no_predicates);

        assert_eq!(sim, 1.0 / 3.0);
        assert_eq!(sim2, 1.0 / 3.0);
    }

    #[test]
    fn test_get_closure_and_ic_map() {
        let spo_cloned = crate::test_utils::test_constants::SPO_FRUITS.clone();
        let mut semsimian = RustSemsimian::new(spo_cloned);
        let test_predicates: Option<HashSet<Predicate>> = Some(
            vec!["related_to"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
        );
        let closure_ic_map: ClosureAndICMap = semsimian.get_closure_and_ic_map(&test_predicates);
        println!("Closure_map from semsimian {:?}", closure_ic_map.closure_map);
        // Closure_table: {"+related_to": {"orange": {"orange", "pear"}, "pear": {"pear", "kiwi"}, "apple": {"apple", "banana"}, "banana": {"banana", "orange"}}}
        println!("ic_map from semsimian  {:?}", closure_ic_map.ic_map);
        // ic_map:  {"+related_to": {"apple": 2.415037499278844, "banana": 2.0, "orange": 2.0, "kiwi": 4.0, "pear": 2.0}}
        assert!(!closure_ic_map.closure_map.is_empty());
        assert!(!closure_ic_map.ic_map.is_empty());
    }

    #[test]
    fn test_resnik_similarity() {
        let spo_cloned = crate::test_utils::test_constants::SPO_FRUITS.clone();
        let mut rs = RustSemsimian::new(spo_cloned);
        let predicates: Option<HashSet<String>> =
            Some(vec!["related_to".to_string()].into_iter().collect());
        let closure_ic_map: ClosureAndICMap = rs.get_closure_and_ic_map(&predicates);
        println!("Closure_map from semsimian {:?}", closure_ic_map.closure_map);
        let (_, sim) =
            rs.resnik_similarity(&"apple".to_string(), &"banana".to_string(), &predicates);
        println!("Do the print{}", sim);
        assert!(sim > 0.0);
        let (_, sim2) =
            rs.resnik_similarity(&"apple".to_string(), &"apple".to_string(), &predicates);
        println!("DO THE print{}", sim2);
        assert_eq!(sim2, 2.415037499278844);
    }

    #[test]
    fn test_all_by_all_pairwise_similarity_with_empty_inputs() {
        let rss = RustSemsimian::new(vec![(
            "apple".to_string(),
            "is_a".to_string(),
            "fruit".to_string(),
        )]);

        let subject_terms: HashSet<TermID> = HashSet::new();
        let object_terms: HashSet<TermID> = HashSet::new();
        let predicates: Option<HashSet<Predicate>> = None;

        let result = rss.all_by_all_pairwise_similarity(&subject_terms, &object_terms, &predicates);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_all_by_all_pairwise_similarity_with_nonempty_inputs() {
        let mut rss = RustSemsimian::new(vec![
            ("apple".to_string(), "is_a".to_string(), "fruit".to_string()),
            ("apple".to_string(), "is_a".to_string(), "food".to_string()),
            ("apple".to_string(), "is_a".to_string(), "item".to_string()),
            ("fruit".to_string(), "is_a".to_string(), "food".to_string()),
            ("fruit".to_string(), "is_a".to_string(), "item".to_string()),
            ("food".to_string(), "is_a".to_string(), "item".to_string()),
        ]);

        let apple = "apple".to_string();
        let fruit = "fruit".to_string();
        let food = "food".to_string();

        let mut subject_terms: HashSet<String> = HashSet::new();
        subject_terms.insert(apple.clone());
        subject_terms.insert(fruit.clone());

        let mut object_terms: HashSet<TermID> = HashSet::new();
        object_terms.insert(fruit.clone());
        object_terms.insert(food.clone());

        let predicates: Option<HashSet<Predicate>> = Some(HashSet::from(["is_a".to_string()]));

        let result = rss.all_by_all_pairwise_similarity(&subject_terms, &object_terms, &predicates);

        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&apple));
        assert!(result.contains_key(&fruit));

        // Apple
        let apple_similarities = result.get(&apple).unwrap();

        assert_eq!(apple_similarities.len(), 2);
        assert!(apple_similarities.contains_key(&fruit));
        assert!(apple_similarities.contains_key(&food));

        // Apple, fruit tests
        let apple_fruit_jaccard = rss.jaccard_similarity(&apple, &fruit, &predicates);
        let (apple_fruit_mica, apple_fruit_resnik) =
            rss.resnik_similarity(&apple, &fruit, &predicates);
        let (
            apple_fruit_resnik_from_similarity,
            apple_fruit_jaccard_from_similarity,
            apple_fruit_phenodigm_from_similarity,
            apple_fruit_mica_from_similarity,
        ) = apple_similarities.get(&fruit).unwrap();

        assert_eq!(*apple_fruit_resnik_from_similarity, apple_fruit_resnik);
        assert_eq!(*apple_fruit_jaccard_from_similarity, apple_fruit_jaccard);
        assert_eq!(
            *apple_fruit_phenodigm_from_similarity,
            (apple_fruit_jaccard * apple_fruit_resnik).sqrt()
        );
        // println!("{apple_similarities:?}");
        // println!("{apple_fruit_mica:?}");

        assert_eq!(*apple_fruit_mica_from_similarity, apple_fruit_mica);

        //Apple, food tests
        let apple_food_jaccard = rss.jaccard_similarity(&apple, &food, &predicates);
        let (apple_food_mcra, apple_food_resnik) =
            rss.resnik_similarity(&apple, &food, &predicates);
        let (
            apple_food_resnik_from_similarity,
            apple_food_jaccard_from_similarity,
            apple_food_phenodigm_from_similarity,
            apple_food_mica_from_similarity,
        ) = apple_similarities.get(&food).unwrap();

        assert_eq!(*apple_food_resnik_from_similarity, apple_food_resnik);
        assert_eq!(*apple_food_jaccard_from_similarity, apple_food_jaccard);
        assert_eq!(
            *apple_food_phenodigm_from_similarity,
            (apple_food_resnik * apple_food_jaccard).sqrt()
        );
        assert_eq!(*apple_food_mica_from_similarity, apple_food_mcra);

        // Fruit
        let fruit_similarities = result.get(&fruit).unwrap();
        let fruit_fruit_jaccard = rss.jaccard_similarity(&fruit, &fruit, &predicates);
        let (fruit_fruit_mica, fruit_fruit_resnik) =
            rss.resnik_similarity(&fruit, &fruit, &predicates);
        let (
            fruit_fruit_resnik_from_similarity,
            fruit_fruit_jaccard_from_similarity,
            fruit_fruit_phenodigm_from_similarity,
            fruit_fruit_mica_from_similarity,
        ) = fruit_similarities.get(&fruit).unwrap();

        // println!("{fruit_similarities:?}");
        // println!("{fruit_fruit_mica:?}");

        assert_eq!(fruit_similarities.len(), 2);
        assert!(fruit_similarities.contains_key(&fruit));
        assert!(fruit_similarities.contains_key(&food));
        // Fruit, fruit tests
        assert_eq!(*fruit_fruit_resnik_from_similarity, fruit_fruit_resnik);
        assert_eq!(*fruit_fruit_jaccard_from_similarity, fruit_fruit_jaccard);
        assert_eq!(
            *fruit_fruit_phenodigm_from_similarity,
            (fruit_fruit_resnik * fruit_fruit_jaccard).sqrt()
        );
        assert_eq!(*fruit_fruit_mica_from_similarity, fruit_fruit_mica);

        // Fruit, food tests
        let fruit_food_jaccard = rss.jaccard_similarity(&fruit, &food, &predicates);
        let (fruit_food_mica, fruit_food_resnik) =
            rss.resnik_similarity(&fruit, &food, &predicates);
        let (
            fruit_food_resnik_from_similarity,
            fruit_food_jaccard_from_similarity,
            fruit_food_phenodigm_from_similarity,
            fruit_food_mica_from_similarity,
        ) = fruit_similarities.get(&food).unwrap();
        assert_eq!(*fruit_food_resnik_from_similarity, fruit_food_resnik);
        assert_eq!(*fruit_food_jaccard_from_similarity, fruit_food_jaccard);
        assert_eq!(
            *fruit_food_phenodigm_from_similarity,
            (fruit_food_resnik * fruit_food_jaccard).sqrt()
        );
        assert_eq!(*fruit_food_mica_from_similarity, fruit_food_mica);

        assert!(!result.contains_key(&food));
        println!("{result:?}");
    }
}
