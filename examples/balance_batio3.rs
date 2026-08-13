//! Compile-tested source for the reaction-balancing example in
//! README.md / README_ja.md (AGENTS.md §25 "examplesをcompile test",
//! §20 "README例は実際の出力から生成する" -- the README's snippet and
//! printed output are copied verbatim from running this).

use gugen::{Composition, Element, balance};

fn formula(composition: &Composition) -> String {
    composition
        .iter()
        .map(|(element, amount)| format!("{}{}", element.symbol(), amount))
        .collect()
}

fn main() -> Result<(), gugen::GugenError> {
    let ba = Element::new("Ba")?;
    let ti = Element::new("Ti")?;
    let o = Element::new("O")?;

    let bao = Composition::new([(ba, 1.0), (o, 1.0)])?;
    let tio2 = Composition::new([(ti, 1.0), (o, 2.0)])?;
    let batio3 = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)])?;

    let reactions = balance(&[bao, tio2], &[batio3])?;
    for reaction in &reactions {
        let lhs: Vec<String> = reaction
            .reactants
            .iter()
            .map(|s| format!("{} {}", s.coefficient, formula(&s.composition)))
            .collect();
        let rhs: Vec<String> = reaction
            .products
            .iter()
            .map(|s| format!("{} {}", s.coefficient, formula(&s.composition)))
            .collect();
        println!("{} -> {}", lhs.join(" + "), rhs.join(" + "));
    }

    Ok(())
}
