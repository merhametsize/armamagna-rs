use std::fmt;

/// Generate repeated combinations of `values` whose sum is equal to `sum`.
#[derive(Debug)]
pub struct RepeatedCombinationsWithSum {
    sum: usize,
    min_cardinality: usize,
    max_cardinality: usize,
    values: Vec<usize>,
    sets: Vec<Vec<usize>>,
}

impl RepeatedCombinationsWithSum {
    /// Constructor
    pub fn new(
        sum: usize,
        min_cardinality: usize,
        max_cardinality: usize,
        values: Vec<usize>,
    ) -> Self {
        debug_assert!(sum > 0 && min_cardinality > 0 && max_cardinality > 0 && !values.is_empty());

        let mut rcs = Self {
            sum,
            min_cardinality,
            max_cardinality,
            values,
            sets: Vec::new(),
        };
        rcs.generate_sets();
        rcs.sets.shrink_to_fit();
        rcs
    }

    /// Returns the number of sets found
    pub fn get_sets_number(&self) -> usize {
        self.sets.len()
    }

    /// Returns a reference to the i-th set
    pub fn get_set(&self, i: usize) -> &Vec<usize> {
        assert!(i < self.sets.len());
        &self.sets[i]
    }

    /// Generate all sets
    fn generate_sets(&mut self) {
        let n = self.values.len();
        let mut solution = vec![0; self.max_cardinality];

        for k in self.min_cardinality..=self.max_cardinality {
            self.combine(n, k, &mut solution, 0, 0, 0);
        }
    }

    /// Recursive combination generation
    fn combine(
        &mut self,
        n: usize,
        k: usize,
        solution: &mut Vec<usize>,
        pos: usize,
        start: usize,
        items_sum: usize,
    ) {
        // Prune
        if items_sum > self.sum {
            return;
        }

        // Terminal case
        debug_assert!(pos <= k);
        if pos == k {
            if items_sum == self.sum {
                self.sets.push(solution[..k].to_vec());
            }
            return;
        }

        // Recursive part
        for i in start..n {
            solution[pos] = self.values[i];
            self.combine(n, k, solution, pos + 1, i, items_sum + self.values[i]);
        }
    }
}

/// Implement `Display` for printing
impl fmt::Display for RepeatedCombinationsWithSum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for set in &self.sets {
            for n in set {
                write!(f, "{} ", n)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_combinations() {
        let values = vec![1, 2, 3];
        let rcs = RepeatedCombinationsWithSum::new(5, 2, 3, values);

        // Check number of sets
        assert!(rcs.get_sets_number() > 0);

        // Check that all sets sum to 5
        for i in 0..rcs.get_sets_number() {
            let s = rcs.get_set(i);
            let sum: usize = s.iter().sum();
            assert_eq!(sum, 5);
            assert!(s.len() >= 2 && s.len() <= 3); // cardinality constraint
        }
    }

    #[test]
    fn test_no_solution() {
        let values = vec![10, 20, 30];
        let rcs = RepeatedCombinationsWithSum::new(5, 1, 3, values);
        assert_eq!(rcs.get_sets_number(), 0); // no possible set sums to 5
    }

    #[test]
    fn test_single_value_multiple_times() {
        let values = vec![1];
        let rcs = RepeatedCombinationsWithSum::new(3, 3, 3, values);

        assert_eq!(rcs.get_sets_number(), 1);
        let s = rcs.get_set(0);
        assert_eq!(s, &vec![1, 1, 1]);
    }
}
