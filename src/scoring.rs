use crate::data::models::{ArrivalEntry, MonthlyIndex};

pub fn compute_monthly_index(data: &[ArrivalEntry]) -> Vec<MonthlyIndex> {
    let mut totals = [0f64; 13];
    let mut counts = [0u32; 13];

    for entry in data {
        let m = entry.month as usize;
        if (1..=12).contains(&m) {
            totals[m] += entry.visitors_thousands as f64;
            counts[m] += 1;
        }
    }

    // Option 1: take the reference, then dereference manually with *
    // .filter(|m| counts[*m as usize] > 0)
    // Option 2: destructure the reference in the parameter itself with &
    // .filter(|&m| counts[m as usize] > 0)
    // Both do the same thing. |&m| just says "I know you're giving me a reference, unwrap it immediately and give me the value directly." It's syntactic sugar.
    let averages: Vec<(u8, f64)> = (1u8..=12)
        .filter(|&m| counts[m as usize] > 0)// filter always reference, even if it is value
        .map(|m| (m, totals[m as usize] / counts[m as usize] as f64)) // always value, map is for modifying the value, transforming it means taking ownwership
        .collect();

    if averages.is_empty() {
        return vec![];
    }

    // .iter on vec returns an iterator over references, so we need to dereference them to get the values.
    // (1u8..=12) iter on range returns an iterator over the values, so we can just use them directly.
    let min = averages.iter().map(|&(_, v)| v).fold(f64::MAX, f64::min);
    let max = averages.iter().map(|(_, v)| *v).fold(f64::MIN, f64::max);

    // vec.iter() -> &T
    // vec.into_iter() -> T ownership is transferred
    // range (1u8..=12) -> T primitive types

    averages
        .iter()
        .map(|(month, avg)| {
            let normalized = if (max - min).abs() < f64::EPSILON {
                5.0
            } else {
                1.0 + 9.0 * (avg - min) / (max - min)
            };
            MonthlyIndex {
                month: *month,
                normalized: (normalized * 10.0).round() / 10.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::ArrivalEntry;

    fn entry(year: i32, month: i8, visitors: i32) -> ArrivalEntry {
        ArrivalEntry {
            year,
            month,
            visitors_thousands: visitors,
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(compute_monthly_index(&[]).is_empty());
    }

    #[test]
    fn single_month_all_equal_returns_midpoint() {
        // When min == max (only one distinct value), the formula would divide
        // by zero — we fall back to 5.0.
        let data = vec![entry(2024, 1, 100)];
        let result = compute_monthly_index(&data);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].month, 1);
        assert_eq!(result[0].normalized, 5.0);
    }

    #[test]
    fn min_month_gets_1_max_month_gets_10() {
        let data = vec![
            entry(2024, 1, 100), // least visitors → should get 1.0
            entry(2024, 6, 550), // most visitors → should get 10.0
        ];
        let result = compute_monthly_index(&data);
        let jan = result.iter().find(|m| m.month == 1).unwrap();
        let jun = result.iter().find(|m| m.month == 6).unwrap();
        assert_eq!(jan.normalized, 1.0);
        assert_eq!(jun.normalized, 10.0);
    }

    #[test]
    fn averages_across_years_before_normalising() {
        // Jan: (100 + 200) / 2 = 150
        // Jun: (300 + 300) / 2 = 300
        // min=150, max=300 → jan=1.0, jun=10.0
        let data = vec![
            entry(2023, 1, 100),
            entry(2024, 1, 200),
            entry(2023, 6, 300),
            entry(2024, 6, 300),
        ];
        let result = compute_monthly_index(&data);
        let jan = result.iter().find(|m| m.month == 1).unwrap();
        let jun = result.iter().find(|m| m.month == 6).unwrap();
        assert_eq!(jan.normalized, 1.0);
        assert_eq!(jun.normalized, 10.0);
    }

    #[test]
    fn middle_value_normalises_correctly() {
        // min=100, max=200, mid=150
        // → 1.0 + 9.0 * (150 - 100) / (200 - 100) = 1.0 + 4.5 = 5.5
        let data = vec![
            entry(2024, 1, 100),
            entry(2024, 6, 150),
            entry(2024, 12, 200),
        ];
        let result = compute_monthly_index(&data);
        let jun = result.iter().find(|m| m.month == 6).unwrap();
        assert_eq!(jun.normalized, 5.5);
    }

    #[test]
    fn invalid_month_zero_is_ignored() {
        let data = vec![
            entry(2024, 0, 999), // month 0 doesn't exist, must be dropped
            entry(2024, 1, 100),
            entry(2024, 12, 200),
        ];
        let result = compute_monthly_index(&data);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|m| m.month != 0));
    }

    #[test]
    fn twelve_months_full_range() {
        // Visitors increase linearly: month 1 = 100, month 12 = 1200
        // So month 1 is always min → 1.0, month 12 is always max → 10.0
        let data: Vec<ArrivalEntry> = (1i8..=12).map(|m| entry(2024, m, m as i32 * 100)).collect();
        let result = compute_monthly_index(&data);
        assert_eq!(result.len(), 12);
        let jan = result.iter().find(|m| m.month == 1).unwrap();
        let dec = result.iter().find(|m| m.month == 12).unwrap();
        assert_eq!(jan.normalized, 1.0);
        assert_eq!(dec.normalized, 10.0);
    }
}
