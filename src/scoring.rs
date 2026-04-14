use crate::data::models::{ArrivalEntry, CityData, Holiday, MonthScore, MonthlyIndex, PricingEntry};

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

pub fn compute_price_index(pricing: &[PricingEntry], month: u8, years: &[i32]) -> Option<f64> {
    let values: Vec<f64> = pricing.iter()
        .filter(|p| p.month == month && (years.is_empty() || years.contains(&p.year)))
        .map(|p| p.price_index)
        .collect();

    if values.is_empty() {
        return None;
    }

    Some(values.iter().sum::<f64>() / values.len() as f64)
}

pub fn price_penalty(index: f64) -> f64 {
    if index <= 70.0       { 0.0 }
    else if index <= 90.0  { 1.0 }
    else if index <= 110.0 { 2.0 }
    else if index <= 130.0 { 3.5 }
    else if index <= 160.0 { 5.5 }
    else                   { 8.0 }
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
        let active = h.occurrences.iter()
            .filter(|o| o.year == year)
            .any(|o| {
                if o.month_start <= o.month_end {
                    month >= o.month_start && month <= o.month_end
                } else {
                    month >= o.month_start || month <= o.month_end
                }
            });

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
    pp: Option<f64>,
) -> f64 {
    let tp = typhoon_penalty(typhoon);
    let raw = match pp {
        Some(pp) => 0.30 * comfort as f64
                  + 0.30 * (11.0 - crowd)
                  + 0.15 * (10.0 - holiday_penalty as f64)
                  + 0.15 * (10.0 - tp)
                  + 0.10 * (10.0 - pp),
        None =>     0.35 * comfort as f64
                  + 0.35 * (11.0 - crowd)
                  + 0.15 * (10.0 - holiday_penalty as f64)
                  + 0.15 * (10.0 - tp),
    };
    (raw.max(1.0).min(10.0) * 10.0).round() / 10.0
}

pub fn compute_monthly_scores(
    city: &CityData,
    year: i32,
    years: &[i32],
) -> Vec<MonthScore> {
    let filtered: Vec<_> = city.arrivals.data.iter()
        .filter(|e| years.is_empty() || years.contains(&e.year))
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
        let pi = compute_price_index(&city.pricing, month, years);
        let pp = pi.map(price_penalty);
        let overall = compute_overall_score(comfort, crowd, hp, &weather.typhoon_risk, pp);

        MonthScore {
            month,
            comfort,
            crowd_index: crowd,
            typhoon_penalty: typhoon_penalty(&weather.typhoon_risk),
            holiday_penalty: hp,
            price_index: pi,
            price_penalty: pp,
            overall,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{ArrivalEntry, ArrivalsData, CityData, Holiday, HolidayOccurrence, PricingEntry, WeatherMonth};

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

    fn make_weather_month(month: u8, heat_index: i32, rain_days: i32, typhoon: &str) -> WeatherMonth {
        WeatherMonth {
            month,
            avg_high_c: 25,
            avg_low_c: 18,
            humidity_pct: 70,
            rainfall_mm: 100,
            rain_days,
            heat_index_c: heat_index,
            typhoon_risk: typhoon.into(),
            notes: String::new(),
        }
    }

    // 12 neutral months: comfort=10, typhoon=none
    fn neutral_weather() -> Vec<WeatherMonth> {
        (1u8..=12).map(|m| make_weather_month(m, 25, 7, "none")).collect()
    }

    fn make_city(arrivals: Vec<ArrivalEntry>, holidays: Vec<Holiday>) -> CityData {
        let mut years: Vec<i32> = arrivals.iter().map(|e| e.year).collect();
        years.sort_unstable();
        years.dedup();
        CityData {
            city: "Test City".into(),
            slug: "test-city".into(),
            weather: neutral_weather(),
            arrivals: ArrivalsData { years, data: arrivals, monthly_index: vec![] },
            holidays,
            notes: vec![],
            pricing: vec![],
            monthly_scores: vec![],
        }
    }

    // ── comfort ───────────────────────────────────────────────────────────────

    #[test]
    fn comfort_extremes() {
        assert_eq!(compute_comfort_score(39, 15), 4); // heat=1, rain=3
        assert_eq!(compute_comfort_score(22, 3), 10); // heat=5, rain=5
    }

    #[test]
    fn comfort_heat_bucket_boundaries() {
        // each boundary: value at threshold stays in current bucket, +1 drops
        assert_eq!(compute_comfort_score(25, 0), 10); // heat=5
        assert_eq!(compute_comfort_score(26, 0), 9);  // heat=4
        assert_eq!(compute_comfort_score(28, 0), 9);  // heat=4
        assert_eq!(compute_comfort_score(29, 0), 8);  // heat=3
        assert_eq!(compute_comfort_score(31, 0), 8);  // heat=3
        assert_eq!(compute_comfort_score(32, 0), 7);  // heat=2
        assert_eq!(compute_comfort_score(34, 0), 7);  // heat=2
        assert_eq!(compute_comfort_score(35, 0), 6);  // heat=1
    }

    #[test]
    fn comfort_rain_bucket_boundaries() {
        assert_eq!(compute_comfort_score(0, 7),  10); // rain=5
        assert_eq!(compute_comfort_score(0, 8),  9);  // rain=4
        assert_eq!(compute_comfort_score(0, 12), 9);  // rain=4
        assert_eq!(compute_comfort_score(0, 13), 8);  // rain=3
        assert_eq!(compute_comfort_score(0, 16), 8);  // rain=3
        assert_eq!(compute_comfort_score(0, 17), 7);  // rain=2
        assert_eq!(compute_comfort_score(0, 20), 7);  // rain=2
        assert_eq!(compute_comfort_score(0, 21), 6);  // rain=1
    }

    // ── overall ───────────────────────────────────────────────────────────────

    #[test]
    fn overall_score_known_values() {
        // 0.35*8 + 0.35*(11-3) + 0.15*10 + 0.15*10 = 2.8+2.8+1.5+1.5 = 8.6
        assert_eq!(compute_overall_score(8, 3.0, 0, "none", None), 8.6);

        // 0.35*6 + 0.35*(11-7) + 0.15*(10-2) + 0.15*(10-2) = 2.1+1.4+1.2+1.2 = 5.9
        assert_eq!(compute_overall_score(6, 7.0, 2, "moderate", None), 5.9);
    }

    #[test]
    fn overall_score_best_case_is_10() {
        // 0.35*10 + 0.35*10 + 0.15*10 + 0.15*10 = 10.0
        assert_eq!(compute_overall_score(10, 1.0, 0, "none", None), 10.0);
    }

    // ── holiday penalty ───────────────────────────────────────────────────────

    #[test]
    fn holiday_penalty_worst_wins_across_multiple() {
        let holidays = vec![
            make_holiday("moderate", 2025, 5, 5), // p=1
            make_holiday("high",     2025, 5, 5), // p=2
            make_holiday("extreme",  2025, 5, 5), // p=3
        ];
        assert_eq!(get_worst_holiday_penalty(&holidays, 5, 2025), 3);
    }

    #[test]
    fn holiday_penalty_multi_month_span() {
        // holiday spans Feb–Apr
        let holidays = vec![make_holiday("very_high", 2025, 2, 4)];
        assert_eq!(get_worst_holiday_penalty(&holidays, 1, 2025), 0);
        assert_eq!(get_worst_holiday_penalty(&holidays, 2, 2025), 2);
        assert_eq!(get_worst_holiday_penalty(&holidays, 3, 2025), 2);
        assert_eq!(get_worst_holiday_penalty(&holidays, 4, 2025), 2);
        assert_eq!(get_worst_holiday_penalty(&holidays, 5, 2025), 0);
    }

    #[test]
    fn holiday_penalty_multi_occurrence_same_year() {
        // Galungan-style: two occurrences in the same year (Mar and Sep)
        let h = Holiday {
            id: "galungan".into(),
            name: "Galungan".into(),
            crowd_impact: "extreme".into(),
            price_impact: "none".into(),
            closure_impact: "none".into(),
            notes: String::new(),
            occurrences: vec![
                HolidayOccurrence { year: 2025, date_start: "2025-03-05".into(), date_end: "2025-03-05".into(), month_start: 3, month_end: 3 },
                HolidayOccurrence { year: 2025, date_start: "2025-09-30".into(), date_end: "2025-09-30".into(), month_start: 9, month_end: 9 },
            ],
        };
        // both months must be active — find() would miss the second occurrence
        assert_eq!(get_worst_holiday_penalty(&[h.clone()], 3, 2025), 3);
        assert_eq!(get_worst_holiday_penalty(&[h.clone()], 9, 2025), 3);
        assert_eq!(get_worst_holiday_penalty(&[h],         6, 2025), 0);
    }

    // ── compute_monthly_scores ────────────────────────────────────────────────

    #[test]
    fn monthly_scores_returns_all_12_months() {
        let city = make_city(vec![], vec![]);
        let scores = compute_monthly_scores(&city, 2025, &[]);
        assert_eq!(scores.len(), 12);
        for month in 1u8..=12 {
            assert!(scores.iter().any(|s| s.month == month), "missing month {month}");
        }
    }

    #[test]
    fn monthly_scores_no_arrivals_crowd_falls_back_to_midpoint() {
        let city = make_city(vec![], vec![]);
        let scores = compute_monthly_scores(&city, 2025, &[]);
        assert!(scores.iter().all(|s| s.crowd_index == 5.0));
    }

    #[test]
    fn monthly_scores_year_range_changes_crowd_index() {
        // 2020: Jan is peak (1000), Jul is off-peak (100)
        // 2023: Jul is peak (1000), Jan is off-peak (100)
        let arrivals = vec![
            entry(2020, 1, 1000), entry(2020, 7, 100),
            entry(2023, 1, 100),  entry(2023, 7, 1000),
        ];
        let city = make_city(arrivals, vec![]);

        let s2020 = compute_monthly_scores(&city, 2025, &[2020]);
        let s2023 = compute_monthly_scores(&city, 2025, &[2023]);

        let jan_2020 = s2020.iter().find(|s| s.month == 1).unwrap();
        let jul_2020 = s2020.iter().find(|s| s.month == 7).unwrap();
        let jan_2023 = s2023.iter().find(|s| s.month == 1).unwrap();
        let jul_2023 = s2023.iter().find(|s| s.month == 7).unwrap();

        // within same year-range Jan vs Jul
        assert!(jan_2020.crowd_index > jul_2020.crowd_index);
        assert!(jul_2023.crowd_index > jan_2023.crowd_index);
        // Jan flips between ranges
        assert!(jan_2020.crowd_index > jan_2023.crowd_index);
        assert!(jul_2023.crowd_index > jul_2020.crowd_index);
    }

    #[test]
    fn monthly_scores_planning_year_changes_holiday_penalty() {
        // extreme holiday in March 2025 only
        let holidays = vec![make_holiday("extreme", 2025, 3, 3)];
        let city = make_city(vec![], holidays);

        let s2025 = compute_monthly_scores(&city, 2025, &[]);
        let s2026 = compute_monthly_scores(&city, 2026, &[]);

        let march_2025 = s2025.iter().find(|s| s.month == 3).unwrap();
        let march_2026 = s2026.iter().find(|s| s.month == 3).unwrap();

        assert_eq!(march_2025.holiday_penalty, 3);
        assert_eq!(march_2026.holiday_penalty, 0);
        assert!(march_2025.overall < march_2026.overall);
    }

    #[test]
    fn monthly_scores_year_range_outside_data_falls_back_to_midpoint() {
        // data only has 2020, filter asks for 2030 → no data → crowd=5.0
        let arrivals = vec![entry(2020, 1, 1000), entry(2020, 7, 100)];
        let city = make_city(arrivals, vec![]);
        let scores = compute_monthly_scores(&city, 2025, &[2030]);
        assert!(scores.iter().all(|s| s.crowd_index == 5.0));
    }

    #[test]
    fn overall_rounds_to_one_decimal() {
        // 0.35*7 + 0.35*(11-4.5) + 0.15*10 + 0.15*10
        // = 2.45 + 2.275 + 1.5 + 1.5 = 7.725 → rounds to 7.7
        let score = compute_overall_score(7, 4.5, 0, "none", None);
        assert_eq!(score, 7.7);
        // verify it's not 7.725 or 7.72 or 7.73
        assert_eq!((score * 10.0).fract(), 0.0);
    }

    #[test]
    fn monthly_scores_year_range_and_planning_year_are_independent() {
        // crowd filter (year_from/to) and planning year should not affect each other
        let arrivals = vec![entry(2020, 1, 1000), entry(2023, 1, 100)];
        let holidays = vec![make_holiday("extreme", 2025, 3, 3)];
        let city = make_city(arrivals, holidays);

        // same planning year, different crowd range → same holiday_penalty, different crowd
        let a = compute_monthly_scores(&city, 2025, &[2020]);
        let b = compute_monthly_scores(&city, 2025, &[2023]);
        let march_a = a.iter().find(|s| s.month == 3).unwrap();
        let march_b = b.iter().find(|s| s.month == 3).unwrap();
        assert_eq!(march_a.holiday_penalty, march_b.holiday_penalty);

        // same crowd range, different planning year → same crowd, different holiday_penalty
        let c = compute_monthly_scores(&city, 2025, &[2020]);
        let d = compute_monthly_scores(&city, 2026, &[2020]);
        let jan_c = c.iter().find(|s| s.month == 1).unwrap();
        let jan_d = d.iter().find(|s| s.month == 1).unwrap();
        assert_eq!(jan_c.crowd_index, jan_d.crowd_index);
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
        let without = compute_overall_score(8, 3.0, 0, "none", None);
        let with_high = compute_overall_score(8, 3.0, 0, "high", None);
        assert!(with_high < without);
        // 0.15 * (10 - 0) - 0.15 * (10 - 6) = 1.5 - 0.6 = 0.9
        assert!((without - with_high - 0.9).abs() < 0.05);
    }

    #[test]
    fn overall_clamped_to_1_10() {
        let low = compute_overall_score(2, 10.0, 3, "high", None);
        let high = compute_overall_score(10, 1.0, 0, "none", None);
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

    // ── pricing ───────────────────────────────────────────────────────────────

    #[test]
    fn price_penalty_thresholds() {
        assert_eq!(price_penalty(65.0),  0.0);
        assert_eq!(price_penalty(85.0),  1.0);
        assert_eq!(price_penalty(100.0), 2.0);
        assert_eq!(price_penalty(120.0), 3.5);
        assert_eq!(price_penalty(145.0), 5.5);
        assert_eq!(price_penalty(170.0), 8.0);
    }

    #[test]
    fn price_index_averages_across_years() {
        let entries = vec![
            PricingEntry { year: 2023, month: 2, price_index: 160.0 },
            PricingEntry { year: 2024, month: 2, price_index: 170.0 },
        ];
        let result = compute_price_index(&entries, 2, &[]).unwrap();
        assert!((result - 165.0).abs() < 0.01);
    }

    #[test]
    fn price_index_filters_by_years() {
        let entries = vec![
            PricingEntry { year: 2023, month: 2, price_index: 160.0 },
            PricingEntry { year: 2024, month: 2, price_index: 170.0 },
        ];
        let r2023 = compute_price_index(&entries, 2, &[2023]).unwrap();
        let r2024 = compute_price_index(&entries, 2, &[2024]).unwrap();
        assert!((r2023 - 160.0).abs() < 0.01);
        assert!((r2024 - 170.0).abs() < 0.01);
    }

    #[test]
    fn price_index_returns_none_for_missing_month() {
        let entries = vec![
            PricingEntry { year: 2024, month: 3, price_index: 110.0 },
        ];
        assert!(compute_price_index(&entries, 2, &[]).is_none());
    }

    #[test]
    fn overall_without_pricing_uses_four_component_formula() {
        // 0.35*8 + 0.35*(11-3) + 0.15*10 + 0.15*10 = 2.8+2.8+1.5+1.5 = 8.6
        let score = compute_overall_score(8, 3.0, 0, "none", None);
        assert!((score - 8.6).abs() < 0.05);
    }

    #[test]
    fn overall_high_price_depresses_score() {
        let cheap     = compute_overall_score(7, 4.0, 0, "none", Some(0.0));
        let expensive = compute_overall_score(7, 4.0, 0, "none", Some(8.0));
        // 0.10 * (10-0) vs 0.10 * (10-8) → 0.8 difference
        assert!(expensive < cheap);
        assert!((cheap - expensive - 0.8).abs() < 0.05);
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
