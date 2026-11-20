//! Calculadoras de organização do treino (documento de evolução v1.1, 8.5 e 8.6).
//! São ferramentas de apoio: não prescrevem cargas.

use super::*;

/// Anilha disponível: peso (g) e quantidade de **pares** (uma de cada lado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plate {
    pub weight_g: i64,
    pub pairs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateResult {
    /// Anilhas de **cada lado**: (peso, quantidade), da mais pesada para a mais leve.
    pub per_side: Vec<(i64, i64)>,
    /// Total montado (barra + anilhas).
    pub total_g: i64,
    /// Montagem exata da carga pedida?
    pub exact: bool,
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// Monta `target_g` com a barra e as anilhas disponíveis, usando o menor número de anilhas.
/// Se não for possível exatamente, devolve a maior carga possível abaixo do alvo (`exact = false`).
/// Erro se o alvo for menor que a barra.
pub fn plates_for(target_g: i64, bar_g: i64, available: &[Plate]) -> AppResult<PlateResult> {
    if target_g < 0 || bar_g < 0 {
        return validation(crate::i18n::tr("Informe valores positivos."));
    }
    if target_g < bar_g {
        return validation(crate::i18n::tr("A carga desejada é menor que o peso da barra."));
    }
    let mut plates: Vec<Plate> = available.iter().copied().filter(|p| p.weight_g > 0 && p.pairs > 0).collect();
    plates.sort_by(|a, b| b.weight_g.cmp(&a.weight_g));
    let side = (target_g - bar_g) / 2;
    if side == 0 || plates.is_empty() {
        return Ok(PlateResult { per_side: vec![], total_g: bar_g, exact: target_g == bar_g });
    }
    let unit = plates.iter().fold(0, |g, p| gcd(g, p.weight_g)).max(1);
    let cap = (side / unit) as usize;
    if cap > 40_000 {
        return validation(crate::i18n::tr("Carga alta demais para calcular."));
    }
    // DP limitada: best[w] = (nº de anilhas, escolha anterior) para alcançar w unidades por lado.
    const NONE: u32 = u32::MAX;
    let mut best: Vec<u32> = vec![NONE; cap + 1];
    let mut from: Vec<(usize, usize)> = vec![(usize::MAX, 0); cap + 1]; // (índice da anilha, w anterior)
    best[0] = 0;
    for (pi, p) in plates.iter().enumerate() {
        let w = (p.weight_g / unit) as usize;
        for _ in 0..p.pairs {
            // 0/1 por cópia (quantidades pequenas), de trás para frente.
            for x in (w..=cap).rev() {
                if best[x - w] != NONE && best[x - w] + 1 < best[x] {
                    best[x] = best[x - w] + 1;
                    from[x] = (pi, x - w);
                }
            }
        }
    }
    let reach = (0..=cap).rev().find(|x| best[*x] != NONE).unwrap_or(0);
    let mut counts = vec![0i64; plates.len()];
    let mut x = reach;
    while x > 0 {
        let (pi, prev) = from[x];
        counts[pi] += 1;
        x = prev;
    }
    let per_side: Vec<(i64, i64)> = plates.iter().zip(&counts).filter(|(_, c)| **c > 0).map(|(p, c)| (p.weight_g, *c)).collect();
    let total_g = bar_g + 2 * (reach as i64 * unit);
    Ok(PlateResult { per_side, total_g, exact: total_g == target_g })
}

/// Passo do aquecimento: porcentagem da carga de trabalho e repetições.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupStep {
    pub percent: i64,
    pub reps: i64,
}

pub const DEFAULT_WARMUP: [WarmupStep; 3] =
    [WarmupStep { percent: 40, reps: 10 }, WarmupStep { percent: 60, reps: 5 }, WarmupStep { percent: 80, reps: 3 }];

/// Séries de aquecimento: carga = % da carga de trabalho, arredondada para `round_g` (para baixo),
/// nunca abaixo da barra (`min_g`) e sempre abaixo da carga de trabalho. Passos repetidos são unidos.
pub fn warmup_sets(work_g: i64, steps: &[WarmupStep], round_g: i64, min_g: i64) -> Vec<(i64, i64)> {
    let round_g = round_g.max(1);
    let mut out: Vec<(i64, i64)> = Vec::new();
    let mut steps = steps.to_vec();
    steps.sort_by_key(|s| s.percent);
    for s in steps {
        if s.percent <= 0 || s.percent >= 100 || s.reps <= 0 {
            continue;
        }
        let raw = work_g * s.percent / 100;
        let load = ((raw / round_g) * round_g).max(min_g);
        if load >= work_g {
            continue;
        }
        if out.last().map(|l| l.0 == load).unwrap_or(false) {
            continue;
        }
        out.push((load, s.reps));
    }
    out
}

pub fn parse_plates(s: &str) -> Vec<Plate> {
    s.split(',')
        .filter_map(|it| {
            let (w, n) = it.split_once(':')?;
            Some(Plate { weight_g: w.trim().parse().ok()?, pairs: n.trim().parse().ok()? })
        })
        .filter(|p| p.weight_g > 0 && p.pairs >= 0)
        .collect()
}

pub fn format_plates(v: &[Plate]) -> String {
    v.iter().map(|p| format!("{}:{}", p.weight_g, p.pairs)).collect::<Vec<_>>().join(",")
}

pub fn default_plates(unit: LoadUnit) -> Vec<Plate> {
    match unit {
        LoadUnit::Kg => [25_000, 20_000, 15_000, 10_000, 5_000, 2_500, 1_250]
            .iter()
            .map(|w| Plate { weight_g: *w, pairs: if *w == 15_000 || *w == 1_250 { 2 } else { 4 } })
            .collect(),
        LoadUnit::Lb => [45.0, 35.0, 25.0, 10.0, 5.0, 2.5]
            .iter()
            .map(|lb| Plate { weight_g: (lb * GRAMS_PER_LB).round() as i64, pairs: 4 })
            .collect(),
    }
}

pub fn default_bar(unit: LoadUnit) -> i64 {
    match unit {
        LoadUnit::Kg => 20_000,
        LoadUnit::Lb => (45.0 * GRAMS_PER_LB).round() as i64,
    }
}

pub fn parse_warmup(s: &str) -> Vec<WarmupStep> {
    let v: Vec<WarmupStep> = s
        .split(',')
        .filter_map(|it| {
            let (p, r) = it.split_once(':')?;
            Some(WarmupStep { percent: p.trim().parse().ok()?, reps: r.trim().parse().ok()? })
        })
        .filter(|s| (1..100).contains(&s.percent) && (1..=50).contains(&s.reps))
        .collect();
    if v.is_empty() { DEFAULT_WARMUP.to_vec() } else { v }
}

pub fn format_warmup(v: &[WarmupStep]) -> String {
    v.iter().map(|s| format!("{}:{}", s.percent, s.reps)).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kg() -> Vec<Plate> {
        default_plates(LoadUnit::Kg)
    }

    #[test]
    fn plates_exact_and_minimal() {
        let r = plates_for(100_000, 20_000, &kg()).unwrap();
        assert!(r.exact);
        assert_eq!(r.per_side.iter().map(|p| p.1).sum::<i64>(), 2); // 40 por lado com 2 anilhas
        let r = plates_for(62_500, 20_000, &kg()).unwrap();
        assert_eq!(r.per_side, vec![(20_000, 1), (1_250, 1)]);
        assert_eq!(r.total_g, 62_500);
        assert!(plates_for(20_000, 20_000, &kg()).unwrap().per_side.is_empty());
    }

    #[test]
    fn plates_impossible_and_limits() {
        // 21 kg não fecha com anilhas de 1,25 kg: mostra o mais próximo abaixo.
        let r = plates_for(21_000, 20_000, &kg()).unwrap();
        assert!(!r.exact);
        assert_eq!(r.total_g, 20_000);
        // Quantidade limitada de pares é respeitada.
        let few = vec![Plate { weight_g: 20_000, pairs: 1 }, Plate { weight_g: 5_000, pairs: 1 }];
        let r = plates_for(100_000, 20_000, &few).unwrap();
        assert_eq!(r.total_g, 70_000);
        assert!(!r.exact);
        assert!(plates_for(10_000, 20_000, &kg()).is_err());
        assert_eq!(parse_plates(&format_plates(&kg())), kg());
    }

    #[test]
    fn warmup() {
        let v = warmup_sets(100_000, &DEFAULT_WARMUP, 2_500, 20_000);
        assert_eq!(v, vec![(40_000, 10), (60_000, 5), (80_000, 3)]);
        // Carga baixa: nunca abaixo da barra e sem repetir a mesma carga.
        let v = warmup_sets(30_000, &DEFAULT_WARMUP, 2_500, 20_000);
        assert_eq!(v, vec![(20_000, 10), (22_500, 3)]);
        assert_eq!(parse_warmup("x"), DEFAULT_WARMUP.to_vec());
        assert_eq!(parse_warmup("50:8,70:4").len(), 2);
    }
}
