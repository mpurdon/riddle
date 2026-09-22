//! Calibration (§27): Platt scaling fitted from labelled (raw, label) pairs.
//! A raw `.80` from the judge is an operating point on a ranking score, not
//! an 80% chance, until a calibration map says so; Mesh thresholds are
//! written against the calibrated score. The labelled set is built from
//! corrections plus audited samples because corrections alone are biased
//! (users only correct what they see, §31).

use riddle_core::{Calibration, CalibrationMethod, Correction, Disposition, SignalEvaluation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlattFit {
    pub a: f64,
    pub b: f64,
    pub iterations: u32,
    pub log_loss: f64,
    pub n: usize,
}

fn logit(p: f64) -> f64 {
    let r = p.clamp(1e-6, 1.0 - 1e-6);
    (r / (1.0 - r)).ln()
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Fit `p_cal = σ(a·logit(raw) + b)` by Newton's method on the log loss with
/// a small ridge term for stability on tiny or separable sets. Uses Platt's
/// smoothed targets so a perfectly separable set does not diverge.
pub fn fit_platt(pairs: &[(f64, bool)]) -> Option<PlattFit> {
    if pairs.len() < 4 {
        return None;
    }
    let n_pos = pairs.iter().filter(|(_, l)| *l).count() as f64;
    let n_neg = pairs.len() as f64 - n_pos;
    if n_pos == 0.0 || n_neg == 0.0 {
        return None;
    }
    let t_pos = (n_pos + 1.0) / (n_pos + 2.0);
    let t_neg = 1.0 / (n_neg + 2.0);
    let xs: Vec<f64> = pairs.iter().map(|(p, _)| logit(*p)).collect();
    let ts: Vec<f64> = pairs
        .iter()
        .map(|(_, l)| if *l { t_pos } else { t_neg })
        .collect();
    let lambda = 1e-3;
    let (mut a, mut b) = (1.0f64, 0.0f64);
    let mut iterations = 0;
    for _ in 0..50 {
        iterations += 1;
        let (mut ga, mut gb, mut haa, mut hab, mut hbb) =
            (lambda * (a - 1.0), lambda * b, lambda, 0.0, lambda);
        for (x, t) in xs.iter().zip(&ts) {
            let p = sigmoid(a * x + b);
            let d = p - t;
            ga += d * x;
            gb += d;
            let w = p * (1.0 - p);
            haa += w * x * x;
            hab += w * x;
            hbb += w;
        }
        let det = haa * hbb - hab * hab;
        if det.abs() < 1e-12 {
            break;
        }
        let da = (hbb * ga - hab * gb) / det;
        let db = (haa * gb - hab * ga) / det;
        a -= da;
        b -= db;
        if da.abs() < 1e-7 && db.abs() < 1e-7 {
            break;
        }
    }
    let mut loss = 0.0;
    for (x, (_, l)) in xs.iter().zip(pairs) {
        let p = sigmoid(a * x + b).clamp(1e-9, 1.0 - 1e-9);
        loss -= if *l { p.ln() } else { (1.0 - p).ln() };
    }
    Some(PlattFit {
        a,
        b,
        iterations,
        log_loss: loss / pairs.len() as f64,
        n: pairs.len(),
    })
}

/// Labelled pairs for one Signal version: raw YES probability and the label.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelledSet {
    pub signal_name: String,
    pub signal_version: u32,
    pub pairs: Vec<(f64, bool)>,
    pub from_corrections: usize,
    pub from_audit: usize,
}

impl LabelledSet {
    pub fn new(name: &str, version: u32) -> Self {
        Self {
            signal_name: name.into(),
            signal_version: version,
            ..Default::default()
        }
    }

    /// Add labels derived from corrections: for each correction whose target
    /// disposition implies a value for this Signal (`expected`), pair it with
    /// the cached evaluation of the corrected message.
    pub fn add_corrections(
        &mut self,
        corrections: &[Correction],
        evaluations: &[SignalEvaluation],
        expected: impl Fn(&Disposition) -> Option<bool>,
    ) {
        for k in corrections.iter().filter(|k| k.retracted_at.is_none()) {
            let Some(to) = &k.to_disposition else {
                continue;
            };
            let Some(label) = expected(to) else { continue };
            let Some(e) = evaluations.iter().find(|e| {
                e.message_id == k.message_id
                    && e.signal_name == self.signal_name
                    && e.signal_version == self.signal_version
            }) else {
                continue;
            };
            let raw = e.distribution.get("YES").copied().unwrap_or(0.0);
            self.pairs.push((raw, label));
            self.from_corrections += 1;
        }
    }

    /// Add audited labels the user gave on a random sample (§31).
    pub fn add_audited(&mut self, labelled: &[(SignalEvaluation, bool)]) {
        for (e, label) in labelled {
            if e.signal_name == self.signal_name && e.signal_version == self.signal_version {
                self.pairs
                    .push((e.distribution.get("YES").copied().unwrap_or(0.0), *label));
                self.from_audit += 1;
            }
        }
    }

    /// Fit and package as the Calibration artifact a Signal version carries.
    /// Returns an uncalibrated artifact when there is too little data, never
    /// an error: an unfitted Signal is usable, at raw operating points.
    pub fn calibrate(&self, engine_identity: &str, held_out_agreement: Option<f64>) -> Calibration {
        match fit_platt(&self.pairs) {
            Some(fit) => Calibration {
                method: CalibrationMethod::Platt,
                labelled_count: self.pairs.len() as u32,
                engine_identity: engine_identity.into(),
                calibrated_at: chrono::Utc::now(),
                platt_a: Some(fit.a),
                platt_b: Some(fit.b),
                held_out_agreement,
            },
            None => Calibration {
                labelled_count: self.pairs.len() as u32,
                ..Calibration::uncalibrated(engine_identity)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    #[test]
    fn the_platt_fit_recovers_a_known_transform() {
        // Labels drawn from σ(2·logit(raw) + 0.5); the fit should find a≈2, b≈0.5.
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let mut pairs = Vec::new();
        for _ in 0..4_000 {
            let raw: f64 = rng.gen_range(0.02..0.98);
            let p_true = sigmoid(2.0 * logit(raw) + 0.5);
            let label = rng.gen::<f64>() < p_true;
            pairs.push((raw, label));
        }
        let fit = fit_platt(&pairs).unwrap();
        assert!((fit.a - 2.0).abs() < 0.3, "a = {}", fit.a);
        assert!((fit.b - 0.5).abs() < 0.3, "b = {}", fit.b);
        let cal = Calibration {
            method: CalibrationMethod::Platt,
            labelled_count: 0,
            engine_identity: "t".into(),
            calibrated_at: chrono::Utc::now(),
            platt_a: Some(fit.a),
            platt_b: Some(fit.b),
            held_out_agreement: None,
        };
        assert!(cal.apply(0.5) > 0.55);
    }

    #[test]
    fn too_little_or_one_sided_data_yields_no_fit() {
        assert!(fit_platt(&[(0.9, true), (0.1, false)]).is_none());
        assert!(fit_platt(&[(0.9, true), (0.8, true), (0.7, true), (0.6, true)]).is_none());
        let set = LabelledSet::new("x", 1);
        assert_eq!(
            set.calibrate("jev:test", None).method,
            CalibrationMethod::None
        );
    }
}
