use crate::data::models::{ArrivalEntry, CityData, Holiday, MonthScore, MonthlyIndex};

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

pub fn compute_comfort_score(heat_index: i32, rain_days: i32) -> i32 {
    let heat = if heat_index <= 25 { 5 }
               else if heat_index <= 28 { 4 }
               else if heat_index <= 31 { 3 }
               else if heat_index <= 34 { 2 }
               else { 1 };

    let rain = if rain_days <= 7 { 5 }
               else if rain_days <= 12 { 4 }
               else if rain_days <= 16 { 3 }
               else if rain_days <= 20 { 2 }
               else { 1 };

    heat + rain
}

fn typhoon_penalty(risk: &str) -> f64 {
    match risk {
        "none"     => 0.0,
        "low"      => 0.5,
        "moderate" => 2.0,
        "high"     => 6.0,
        _          => 0.0,
    }
}

pub fn get_worst_holiday_penalty(holidays: &[Holiday], month: u8, year: i32) -> i32 {
    let mut worst = 0i32;
    for h in holidays {
        let active = h.occurrences.iter().find(|o| o.year == year)
            .map(|o| {
                if o.month_start <= o.month_end {
                    month >= o.month_start && month <= o.month_end
                } else {
                    month >= o.month_start || month <= o.month_end
                }
            })
            .unwrap_or(false);

        if active {
            let p = match h.crowd_impact.as_str() {
                "extreme"   => 3,
                "very_high" => 2,
                "high"      => 2,
                "moderate"  => 1,
                _           => 0,
            };
            worst = worst.max(p);
        }
    }
    worst
}

pub fn compute_overall_score(
    comfort: i32,
    crowd: f64,
    holiday_penalty: i32,
    typhoon: &str,
) -> f64 {
    let tp = typhoon_penalty(typhoon);
    let raw = 0.35 * comfort as f64
            + 0.35 * (11.0 - crowd)
            + 0.15 * (10.0 - holiday_penalty as f64)
            + 0.15 * (10.0 - tp);
    let clamped = raw.max(1.0).min(10.0);
    (clamped * 10.0).round() / 10.0
}

pub fn compute_monthly_scores(
    city: &CityData,
    year: i32,
    year_from: Option<i32>,
    year_to: Option<i32>,
) -> Vec<MonthScore> {
    let filtered: Vec<_> = city.arrivals.data.iter()
        .filter(|e| {
            year_from.map_or(true, |f| e.year >= f) &&
            year_to.map_or(true, |t| e.year <= t)
        })
        .cloned()
        .collect();

    let monthly_index = compute_monthly_index(&filtered);

    (1u8..=12).map(|month| {
        let weather = city.weather.iter().find(|w| w.month == month).unwrap();
        let crowd = monthly_index.iter()
            .find(|m| m.month == month)
            .map(|m| m.normalized)
            .unwrap_or(5.0);

        let comfort = compute_comfort_score(weather.heat_index_c, weather.rain_days);
        let hp = get_worst_holiday_penalty(&city.holidays, month, year);
        let overall = compute_overall_score(comfort, crowd, hp, &weather.typhoon_risk);

        MonthScore {
            month,
            comfort,
            crowd_index: crowd,
            typhoon_penalty: typhoon_penalty(&weather.typhoon_risk),
            holiday_penalty: hp,
            overall,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{ArrivalEntry, Holiday, HolidayOccurrence};

    fn make_holiday(crowd_impact: &str, year: i32, month_start: u8, month_end: u8) -> Holiday {
        Holiday {
            id: "test".into(),
            name: "Test Holiday".into(),
            crowd_impact: crowd_impact.into(),
            price_impact: "none".into(),
            closure_impact: "none".into(),
            notes: String::new(),
            occurrences: vec![HolidayOccurrence {
                year,
                date_start: format!("{year}-{month_start:02}-01"),
                date_end: format!("{year}-{month_end:02}-01"),
                month_start,
                month_end,
            }],
        }
    }

    fn entry(year: i32, month: i8, visitors: i32) -> ArrivalEntry {
        ArrivalEntry {
            year,
            month,
            visitors_thousands: visitors,
        }
    }

    #[test]
    fn comfort_extremes() {
        assert_eq!(compute_comfort_score(39, 15), 4); // heat=1, rain=3
        assert_eq!(compute_comfort_score(22, 3), 10); // heat=5, rain=5
    }

    #[test]
    fn typhoon_penalty_values() {
        assert_eq!(typhoon_penalty("none"), 0.0);
        assert_eq!(typhoon_penalty("low"), 0.5);
        assert_eq!(typhoon_penalty("moderate"), 2.0);
        assert_eq!(typhoon_penalty("high"), 6.0);
        assert_eq!(typhoon_penalty("unknown"), 0.0);
    }

    #[test]
    fn overall_high_typhoon_depresses_score() {
        let without = compute_overall_score(8, 3.0, 0, "none");
        let with_high = compute_overall_score(8, 3.0, 0, "high");
        assert!(with_high < without);
        // 0.15 * (10 - 0) - 0.15 * (10 - 6) = 1.5 - 0.6 = 0.9
        assert!((without - with_high - 0.9).abs() < 0.05);
    }

    #[test]
    fn overall_clamped_to_1_10() {
        let low = compute_overall_score(2, 10.0, 3, "high");
        let high = compute_overall_score(10, 1.0, 0, "none");
        assert!(low >= 1.0);
        assert!(high <= 10.0);
    }

    #[test]
    fn holiday_penalty_standard_month() {
        let holidays = vec![make_holiday("extreme", 2025, 2, 2)];
        assert_eq!(get_worst_holiday_penalty(&holidays, 2, 2025), 3);
        assert_eq!(get_worst_holiday_penalty(&holidays, 3, 2025), 0);
    }

    #[test]
    fn holiday_penalty_dec_jan_wrap() {
        // month_start=12, month_end=1 — spans Dec and Jan
        let holidays = vec![make_holiday("very_high", 2025, 12, 1)];
        assert_eq!(get_worst_holiday_penalty(&holidays, 12, 2025), 2);
        assert_eq!(get_worst_holiday_penalty(&holidays, 1, 2025), 2);
        assert_eq!(get_worst_holiday_penalty(&holidays, 6, 2025), 0);
    }

    #[test]
    fn holiday_penalty_wrong_year_ignored() {
        let holidays = vec![make_holiday("extreme", 2024, 3, 3)];
        assert_eq!(get_worst_holiday_penalty(&holidays, 3, 2025), 0);
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
