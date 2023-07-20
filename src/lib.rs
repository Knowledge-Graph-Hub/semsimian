use pyo3::prelude::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    sync::{Arc, Mutex, RwLock}, any::Any,
};

pub mod similarity;

pub mod utils;
use rayon::prelude::*;

mod test_utils;

use std::fmt;

use similarity::{
    calculate_cosine_similarity_for_nodes, calculate_max_information_content,
    calculate_phenomizer_score,
};
use utils::{
    convert_list_of_tuples_to_hashmap, expand_term_using_closure,
    generate_progress_bar_of_length_and_message, predicate_set_to_key,
};

// change to "pub" because it is easier for testing
pub type Predicate = String;
pub type TermID = String;
pub type PredicateSetKey = String;
pub type Jaccard = f64;
pub type Resnik = f64;
pub type Phenodigm = f64;
pub type Cosine = f64;
pub type MostInformativeAncestors = HashSet<TermID>;
type SimilarityMap =
    HashMap<TermID, HashMap<TermID, (Jaccard, Resnik, Phenodigm, MostInformativeAncestors)>>;
type Embeddings = Vec<(String, Vec<f64>)>;

#[derive(Clone)]
pub struct RustSemsimian {
    spo: Vec<(TermID, Predicate, TermID)>,

    ic_map: HashMap<PredicateSetKey, HashMap<TermID, f64>>,
    // ic_map is something like {("is_a_+_part_of"), {"GO:1234": 1.234}}
    closure_map: HashMap<PredicateSetKey, HashMap<TermID, HashSet<TermID>>>,
    // closure_map is something like {("is_a_+_part_of"), {"GO:1234": {"GO:1234", "GO:5678"}}}
    embeddings: Embeddings,
    term_pairwise_similarity_attributes: Option<Vec<String>>,
}

impl RustSemsimian {
    // TODO: this is tied directly to Oak, and should be made more generic
    // TODO: also, we should support loading "custom" ic
    // TODO: generate ic map and closure map using (spo).
    pub fn new(
        spo: Vec<(TermID, Predicate, TermID)>,
        term_pairwise_similarity_attributes: Option<Vec<String>>,
    ) -> RustSemsimian {
        RustSemsimian {
            spo,
            term_pairwise_similarity_attributes,
            ic_map: HashMap::new(),
            closure_map: HashMap::new(),
            embeddings: Vec::new(),
        }
    }

    pub fn update_closure_and_ic_map(&mut self, predicates: &Option<HashSet<Predicate>>) {
        let predicate_set_key = predicate_set_to_key(predicates);
        let (this_closure_map, this_ic_map) =
            convert_list_of_tuples_to_hashmap(&self.spo, predicates);
        self.closure_map.insert(
            predicate_set_key.clone(),
            this_closure_map.get(&predicate_set_key).unwrap().clone(),
        );
        self.ic_map.insert(
            predicate_set_key.clone(),
            this_ic_map.get(&predicate_set_key).unwrap().clone(),
        );
    }

    pub fn load_embeddings(&mut self, embeddings_file: &str) {
        if let Ok(file) = File::open(embeddings_file) {
            let reader = BufReader::new(file);

            let mut embeddings: Vec<(String, Vec<f64>)> = Vec::new();
            let mut lines = reader.lines();

            // Skip the header row
            lines.next();

            for line in lines.flatten() {
                let values: Vec<&str> = line.split('\t').collect();
                let curie = values[0].to_string();
                let embedding: Vec<f64> = values[1..]
                    .iter()
                    .filter_map(|value| value.parse().ok())
                    .collect();
                embeddings.push((curie, embedding));
            }

            self.embeddings = embeddings;
        }
    }

    pub fn jaccard_similarity(
        &self,
        term1: &str,
        term2: &str,
        predicates: &Option<HashSet<Predicate>>,
    ) -> f64 {
        let apple_set = expand_term_using_closure(term1, &self.closure_map, predicates);
        let fruit_set = expand_term_using_closure(term2, &self.closure_map, predicates);

        let intersection = apple_set.intersection(&fruit_set).count() as f64;
        let union = apple_set.union(&fruit_set).count() as f64;
        intersection / union
    }

    pub fn resnik_similarity(
        &self,
        term1: &str,
        term2: &str,
        predicates: &Option<HashSet<Predicate>>,
    ) -> (HashSet<String>, f64) {
        calculate_max_information_content(&self.closure_map, &self.ic_map, term1, term2, predicates)
    }

    pub fn cosine_similarity(&self, term1: &str, term2: &str, embeddings: &Embeddings) -> f64 {
        calculate_cosine_similarity_for_nodes(embeddings, term1, term2).unwrap()
    }

    pub fn all_by_all_pairwise_similarity(
        &self,
        subject_terms: &HashSet<TermID>,
        object_terms: &HashSet<TermID>,
        minimum_jaccard_threshold: &Option<f64>,
        minimum_resnik_threshold: &Option<f64>,
        predicates: &Option<HashSet<Predicate>>,
    ) -> SimilarityMap {
        let self_shared = Arc::new(RwLock::new(self.clone()));
        let pb = generate_progress_bar_of_length_and_message(
            (subject_terms.len() * object_terms.len()) as u64,
            "Building all X all pairwise similarity:",
        );

        let similarity_map: SimilarityMap = subject_terms
            .par_iter() // parallelize computations
            .map(|subject| {
                let mut subject_similarities: HashMap<
                    TermID,
                    (Jaccard, Resnik, Phenodigm, MostInformativeAncestors),
                > = HashMap::new();
                for object in object_terms.iter() {
                    let self_read = self_shared.read().unwrap();
                    let jaccard_sim = self_read.jaccard_similarity(subject, object, predicates);
                    let (mica, resnik_sim) =
                        self_read.resnik_similarity(subject, object, predicates);

                    if minimum_jaccard_threshold.map_or(true, |t| jaccard_sim > t)
                        && minimum_resnik_threshold.map_or(true, |t| resnik_sim > t)
                    {
                        subject_similarities.insert(
                            object.clone(),
                            (
                                jaccard_sim,
                                resnik_sim,
                                (resnik_sim * jaccard_sim).sqrt(),
                                mica,
                            ),
                        );
                    }

                    pb.inc(1);
                }
                (subject.clone(), subject_similarities)
            })
            .collect();
        pb.finish_with_message("done");
        similarity_map
    }

    pub fn all_by_all_pairwise_similarity_with_output(
        &self,
        subject_terms: &HashSet<TermID>,
        object_terms: &HashSet<TermID>,
        minimum_jaccard_threshold: &Option<f64>,
        minimum_resnik_threshold: &Option<f64>,
        predicates: &Option<HashSet<Predicate>>,
        outfile: &Option<&str>,
    ) {
        let self_shared = Arc::new(RwLock::new(self.clone()));
        let pb = generate_progress_bar_of_length_and_message(
            (subject_terms.len() * object_terms.len()) as u64,
            "Building all X all pairwise similarity:",
        );
        let outfile = outfile.unwrap_or("similarity_map.tsv");
        let file = File::create(outfile).unwrap();
        let writer = Arc::new(Mutex::new(BufWriter::new(file)));
        let column_names: Vec<&str> = vec![
            "subject_id",
            "object_id",
            "jaccard_similarity",
            "ancestor_information_content",
            "phenodigm_score",
            "cosine_similarity",
            "ancestor_id",
        ];
        let mut output_map: BTreeMap<&str, Box<dyn Any>> = BTreeMap::new();
        if let Some(output_columns_vector) =
            &self.term_pairwise_similarity_attributes
        {
            for name in output_columns_vector {
                output_map.insert(name, Box::new(None::<String>));
            }
        } else {
            for name in &column_names {
                output_map.insert(name, Box::new(None::<String>));
            }
        }

        let column_names_as_str = output_map.keys().cloned().collect::<Vec<&str>>().join("\t");

        // Write the column names to the TSV file
        let mut writer_1 = writer.lock().unwrap();
        writeln!(&mut *writer_1, "{column_names_as_str}").unwrap();
        drop(writer_1);

        subject_terms
            .par_iter() // parallelize computations
            .for_each(|subject_id| {
                for object_id in object_terms.iter() {
                    let self_read = self_shared.read().unwrap();
                    let jaccard_similarity =
                        self_read.jaccard_similarity(subject_id, object_id, predicates);
                    let (ancestor_id, ancestor_information_content) =
                        self_read.resnik_similarity(subject_id, object_id, predicates);
                    let cosine_similarity = match &self_read.embeddings.is_empty() {
                        true => std::f64::NAN,
                        false => self_read.cosine_similarity(
                            subject_id,
                            object_id,
                            &self_read.embeddings,
                        ),
                    };
                    // TODO: This block of code is repeated and needs to be addressed.
                    let mut output_map: BTreeMap<&str, Box<dyn Any>> = BTreeMap::new();

                    if let Some(output_columns_vector) =
                        &self_read.term_pairwise_similarity_attributes
                    {
                        for name in output_columns_vector {
                            output_map.insert(name, Box::new(None::<String>));
                        }
                    } else {
                        for name in &column_names {
                            output_map.insert(name, Box::new(None::<String>));
                        }
                    }

                    // Overwrite output_map values with variable values that correspond to the keys if they exist
                    if let Some(value) = output_map.get_mut("subject_id") {
                        *value = Box::new(Some(subject_id.to_string()));
                    }
                    if let Some(value) = output_map.get_mut("object_id") {
                        *value = Box::new(Some(object_id.to_string()));
                    }
                    if let Some(value) = output_map.get_mut("jaccard_similarity") {
                        *value = Box::new(Some(jaccard_similarity));
                    }
                    if let Some(value) = output_map.get_mut("ancestor_information_content") {
                        *value = Box::new(Some(ancestor_information_content));
                    }
                    if let Some(value) = output_map.get_mut("phenodigm_score") {
                        *value = Box::new(Some(
                            (ancestor_information_content * jaccard_similarity).sqrt(),
                        ));
                    }
                    if let Some(value) = output_map.get_mut("cosine_similarity") {
                        *value = Box::new(Some(cosine_similarity));
                    }
                    if let Some(value) = output_map.get_mut("ancestor_id") {
                        *value = Box::new(Some(
                            ancestor_id.into_iter().collect::<Vec<String>>().join(", "),
                        ));
                    }

                    if minimum_jaccard_threshold.map_or(true, |t| jaccard_similarity > t)
                        && minimum_resnik_threshold
                            .map_or(true, |t| ancestor_information_content > t)
                    {
                        // Write the line to the TSV file
                        let mut writer_2 = writer.lock().unwrap();
                        writeln!(
                            &mut *writer_2,
                            "{}",
                            output_map
                                .values()
                                .map(|value| {
                                    match value.downcast_ref::<Option<String>>() {
                                        Some(Some(s)) => s,
                                        _ => "",
                                    }
                                })
                                .collect::<Vec<&str>>()
                                .join("\t")
                        )
                        .unwrap();
                    }

                    pb.inc(1);
                }
            });

        pb.finish_with_message("done");
    }

    // TODO: make this predicate aware, and make it work with the new closure map
    pub fn phenomizer_score(
        map: HashMap<String, HashMap<String, f64>>,
        entity1: HashSet<String>,
        entity2: HashSet<String>,
    ) -> PyResult<f64> {
        Ok(calculate_phenomizer_score(map, entity1, entity2))
    }
}

#[pyclass]
pub struct Semsimian {
    ss: RustSemsimian,
}

#[pymethods]
impl Semsimian {
    #[new]
    fn new(
        spo: Vec<(TermID, Predicate, TermID)>,
        term_pairwise_similarity_attributes: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let ss = RustSemsimian::new(spo, term_pairwise_similarity_attributes);
        Ok(Semsimian { ss })
    }

    fn jaccard_similarity(
        &mut self,
        term1: TermID,
        term2: TermID,
        predicates: Option<HashSet<Predicate>>,
    ) -> PyResult<f64> {
        self.ss.update_closure_and_ic_map(&predicates);
        Ok(self.ss.jaccard_similarity(&term1, &term2, &predicates))
    }

    fn cosine_similarity(
        &mut self,
        term1: TermID,
        term2: TermID,
        embeddings_file: &str,
    ) -> PyResult<f64> {
        self.ss.load_embeddings(embeddings_file);
        Ok(self
            .ss
            .cosine_similarity(&term1, &term2, &self.ss.embeddings))
    }

    fn resnik_similarity(
        &mut self,
        term1: TermID,
        term2: TermID,
        predicates: Option<HashSet<Predicate>>,
    ) -> PyResult<(HashSet<String>, f64)> {
        self.ss.update_closure_and_ic_map(&predicates);
        Ok(self.ss.resnik_similarity(&term1, &term2, &predicates))
    }

    fn all_by_all_pairwise_similarity(
        &mut self,
        subject_terms: HashSet<TermID>,
        object_terms: HashSet<TermID>,
        minimum_jaccard_threshold: Option<f64>,
        minimum_resnik_threshold: Option<f64>,
        predicates: Option<HashSet<Predicate>>,
    ) -> SimilarityMap {
        // first make sure we have the closure and ic map for the given predicates
        self.ss.update_closure_and_ic_map(&predicates);

        self.ss.all_by_all_pairwise_similarity(
            &subject_terms,
            &object_terms,
            &minimum_jaccard_threshold,
            &minimum_resnik_threshold,
            &predicates,
        )
    }

    fn all_by_all_pairwise_similarity_quick(
        &mut self,
        subject_terms: HashSet<TermID>,
        object_terms: HashSet<TermID>,
        minimum_jaccard_threshold: Option<f64>,
        minimum_resnik_threshold: Option<f64>,
        predicates: Option<HashSet<Predicate>>,
        embeddings_file: Option<&str>,
        outfile: Option<&str>,
    ) -> PyResult<()> {
        // first make sure we have the closure and ic map for the given predicates
        self.ss.update_closure_and_ic_map(&predicates);
        if let Some(file) = embeddings_file {
            self.ss.load_embeddings(file);
        }

        self.ss.all_by_all_pairwise_similarity_with_output(
            &subject_terms,
            &object_terms,
            &minimum_jaccard_threshold,
            &minimum_resnik_threshold,
            &predicates,
            &outfile,
        );
        Ok(())
    }

    fn get_spo(&self) -> PyResult<Vec<(TermID, Predicate, TermID)>> {
        Ok(self.ss.spo.to_vec())
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
    use crate::{test_utils::test_constants::SPO_FRUITS, RustSemsimian};
    use std::{
        collections::HashSet,
        io::{BufRead, BufReader},
    };

    #[test]
    fn test_jaccard_similarity() {
        let spo_cloned = SPO_FRUITS.clone();
        let predicates: Option<HashSet<Predicate>> = Some(
            vec!["related_to"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
        );
        let no_predicates: Option<HashSet<Predicate>> = None;
        let mut ss = RustSemsimian::new(spo_cloned, None);
        ss.update_closure_and_ic_map(&predicates);
        println!("Closure table for ss  {:?}", ss.closure_map);
        //Closure table: {"+related_to": {"apple": {"banana", "apple"}, "banana": {"orange", "banana"}, "pear": {"kiwi", "pear"}, "orange": {"orange", "pear"}}}
        let apple = "apple".to_string();
        let banana = "banana".to_string();
        let sim = ss.jaccard_similarity(&apple, &banana, &predicates);
        let sim2 = ss.jaccard_similarity(&apple, &banana, &no_predicates);

        assert_eq!(sim, 1.0 / 3.0);
        assert_eq!(sim2, 1.0 / 3.0);
    }

    #[test]
    fn test_get_closure_and_ic_map() {
        let spo_cloned = SPO_FRUITS.clone();
        let mut semsimian = RustSemsimian::new(spo_cloned, None);
        println!("semsimian after initialization: {semsimian:?}");
        let test_predicates: Option<HashSet<Predicate>> = Some(
            vec!["related_to"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
        );
        semsimian.update_closure_and_ic_map(&test_predicates);
        assert!(!semsimian.closure_map.is_empty());
        assert!(!semsimian.ic_map.is_empty());
    }

    #[test]
    fn test_resnik_similarity() {
        let spo_cloned = SPO_FRUITS.clone();
        let mut rs = RustSemsimian::new(spo_cloned, None);
        let predicates: Option<HashSet<String>> =
            Some(vec!["related_to".to_string()].into_iter().collect());
        rs.update_closure_and_ic_map(&predicates);
        println!("Closure_map from semsimian {:?}", rs.closure_map);
        let (_, sim) = rs.resnik_similarity("apple", "banana", &predicates);
        println!("DO THE print{sim}");
        assert_eq!(sim, 1.3219280948873622);
    }

    #[test]
    fn test_all_by_all_pairwise_similarity_with_empty_inputs() {
        let rss = RustSemsimian::new(
            vec![("apple".to_string(), "is_a".to_string(), "fruit".to_string())],
            None,
        );

        let subject_terms: HashSet<TermID> = HashSet::new();
        let object_terms: HashSet<TermID> = HashSet::new();
        let predicates: Option<HashSet<Predicate>> = None;

        let result = rss.all_by_all_pairwise_similarity(
            &subject_terms,
            &object_terms,
            &Some(0.0),
            &Some(0.0),
            &predicates,
        );

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_all_by_all_pairwise_similarity_with_nonempty_inputs() {
        let mut rss = RustSemsimian::new(
            vec![
                ("apple".to_string(), "is_a".to_string(), "fruit".to_string()),
                ("apple".to_string(), "is_a".to_string(), "food".to_string()),
                ("apple".to_string(), "is_a".to_string(), "item".to_string()),
                ("fruit".to_string(), "is_a".to_string(), "food".to_string()),
                ("fruit".to_string(), "is_a".to_string(), "item".to_string()),
                ("food".to_string(), "is_a".to_string(), "item".to_string()),
            ],
            None,
        );

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
        rss.update_closure_and_ic_map(&predicates);
        let result = rss.all_by_all_pairwise_similarity(
            &subject_terms,
            &object_terms,
            &Some(0.0),
            &Some(0.0),
            &predicates,
        );

        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&apple));
        // assert!(result.contains_key(&fruit));

        // Apple
        let apple_similarities = result.get(&apple).unwrap();
        // println!("{apple_similarities:?}");
        assert_eq!(apple_similarities.len(), 1);
        assert!(apple_similarities.contains_key(&fruit));
        assert!(!apple_similarities.contains_key(&food)); // Since resnik <= threshold

        // Apple, fruit tests
        let apple_fruit_jaccard = rss.jaccard_similarity(&apple, &fruit, &predicates);
        let (apple_fruit_mica, apple_fruit_resnik) =
            rss.resnik_similarity(&apple, &fruit, &predicates);
        let (
            apple_fruit_jaccard_from_similarity,
            apple_fruit_resnik_from_similarity,
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

        assert_eq!(0.0, apple_food_resnik);
        assert_eq!(0.3333333333333333, apple_food_jaccard);
        assert_eq!(HashSet::from(["item".to_string()]), apple_food_mcra);

        // Fruit
        let fruit_similarities = result.get(&fruit).unwrap();
        let fruit_fruit_jaccard = rss.jaccard_similarity(&fruit, &fruit, &predicates);
        let (fruit_fruit_mica, fruit_fruit_resnik) =
            rss.resnik_similarity(&fruit, &fruit, &predicates);
        let (
            fruit_fruit_jaccard_from_similarity,
            fruit_fruit_resnik_from_similarity,
            fruit_fruit_phenodigm_from_similarity,
            fruit_fruit_mica_from_similarity,
        ) = fruit_similarities.get(&fruit).unwrap();

        // println!("{fruit_similarities:?}");
        // println!("{fruit_fruit_mica:?}");

        assert_eq!(fruit_similarities.len(), 1);
        assert!(fruit_similarities.contains_key(&fruit));
        assert!(!fruit_similarities.contains_key(&food)); // Since Resnik <= threshold

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
        assert_eq!(0.0, fruit_food_resnik);
        assert_eq!(0.5, fruit_food_jaccard);
        assert_eq!(HashSet::from(["item".to_string()]), fruit_food_mica);
        assert!(!result.contains_key(&food)); // Since Resnik <= threshold
        println!("all_by_all_pairwise_similarity result: {result:?}");
    }

    #[test]
    fn test_all_by_all_pairwise_similarity_with_output() {
        let output_columns = crate::test_utils::test_constants::OUTPUT_COLUMNS_VECTOR.clone();
        let mut rss = RustSemsimian::new(SPO_FRUITS.clone(), Some(output_columns));
        let banana = "banana".to_string();
        let apple = "apple".to_string();
        let pear = "pear".to_string();
        let outfile = Some("tests/data/output/similarity_test_output.tsv");
        let embeddings_file = Some("tests/data/test_embeddings.tsv");
        

        let mut subject_terms: HashSet<String> = HashSet::new();
        subject_terms.insert(banana);
        subject_terms.insert(apple.clone());

        let mut object_terms: HashSet<TermID> = HashSet::new();
        object_terms.insert(apple);
        object_terms.insert(pear);

        let predicates: Option<HashSet<Predicate>> =
            Some(HashSet::from(["related_to".to_string()]));
        rss.update_closure_and_ic_map(&predicates);
        rss.load_embeddings(embeddings_file.unwrap());
        rss.all_by_all_pairwise_similarity_with_output(
            &subject_terms,
            &object_terms,
            &Some(0.0),
            &Some(0.0),
            &predicates,
            &outfile,
        );

        // Read the outfile and count the number of lines
        let file = File::open(outfile.unwrap()).unwrap();
        let reader = BufReader::new(file);

        let line_count = reader.lines().count();
        // Assert that the line count is 3 (including the header)
        assert_eq!(line_count, 3);
    }

    #[test]
    fn test_resnik_using_bfo() {
        let spo = crate::test_utils::test_constants::BFO_SPO.clone();
        let mut rss = RustSemsimian::new(spo, None);

        let predicates: Option<HashSet<Predicate>> = Some(HashSet::from([
            "rdfs:subClassOf".to_string(),
            "BFO:0000050".to_string(),
        ]));

        rss.update_closure_and_ic_map(&predicates);
        // println!("IC_map from semsimian {:?}", rss.ic_map);
        let (_, sim) = rss.resnik_similarity("BFO:0000040", "BFO:0000002", &predicates);
        println!("DO THE print {sim}");
        assert_eq!(sim, 0.4854268271702417);
    }

    #[test]
    fn test_cosine_using_bfo() {
        let spo = crate::test_utils::test_constants::BFO_SPO.clone();
        let mut rss = RustSemsimian::new(spo, None);
        let embeddings_file = Some("tests/data/bfo_embeddings.tsv");

        rss.load_embeddings(embeddings_file.unwrap());

        let cosine_similarity =
            rss.cosine_similarity("BFO:0000040", "BFO:0000002", &rss.embeddings);
        println!("DO THE print {cosine_similarity}");
        assert_eq!(cosine_similarity, 0.09582515104047208);
    }
}
