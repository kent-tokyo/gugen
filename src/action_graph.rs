//! A compact, deterministic ActionGraph representation for synthesis plans.
//!
//! This is the representation layer inspired by Andrello, Alabi, and
//! Billinge, arXiv:2512.02947 (2025), not a predictor.  It keeps gugen's
//! existing chemistry and process objects as the source of truth and adds a
//! graph view that callers can serialize, compare, or feed to their own
//! retrieval code.

use crate::process::{PlannedStep, ProcessStep, StepRequirement};
use crate::reaction::BalancedReaction;
use crate::report::SynthesisPlan;
use thiserror::Error;

/// High-level operation labels used by the ActionGraph representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ActionKind {
    StartingSynthesis,
    MixingOperation,
    MillingOperation,
    ShapingOperation,
    HeatingOperation,
    QuenchingOperation,
    CharacterizationOperation,
}

/// One node in an [`ActionGraph`]. Chemical nodes retain their exact
/// composition; operation nodes retain the plan's requirement classification
/// so an unresolved template step cannot be mistaken for a confirmed action.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActionNode {
    Chemical {
        composition: crate::Composition,
        role: ChemicalRole,
    },
    Operation {
        kind: ActionKind,
        step_index: usize,
        requirement: StepRequirement,
    },
}

/// The role of a chemical node in the graph. Reactant and product nodes are
/// intentionally separate even when their compositions are equal, matching
/// the paper's pre/post-operation node distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ChemicalRole {
    Precursor,
    Product,
}

/// Errors produced while converting a plan into an ActionGraph.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ActionGraphError {
    #[error("a synthesis plan without a balanced reaction has no chemical edges to graph")]
    MissingBalancedReaction,
    #[error("an ActionGraph needs at least one process step")]
    EmptyProcess,
}

/// A directed acyclic graph of chemical nodes and process-operation nodes.
/// `edges` stores `(source, destination)` node indexes in deterministic node
/// order; `adjacency_matrix` exposes the same topology for lightweight
/// similarity or retrieval code without bringing a graph dependency into the
/// core crate.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionGraph {
    nodes: Vec<ActionNode>,
    edges: Vec<(usize, usize)>,
}

impl ActionGraph {
    /// Converts an existing balanced reaction and ordered process steps into
    /// a graph. Reactants feed the first operation, operations form a linear
    /// process chain, and the last operation feeds products. This deliberately
    /// does not infer unreported intermediate phases.
    pub fn from_reaction_and_steps(
        reaction: &BalancedReaction,
        steps: &[PlannedStep],
    ) -> Result<Self, ActionGraphError> {
        if steps.is_empty() {
            return Err(ActionGraphError::EmptyProcess);
        }

        let mut nodes = reaction
            .reactants()
            .iter()
            .map(|species| ActionNode::Chemical {
                composition: species.composition.clone(),
                role: ChemicalRole::Precursor,
            })
            .collect::<Vec<_>>();
        let precursor_count = nodes.len();
        let operation_start = nodes.len();
        nodes.extend(
            steps
                .iter()
                .enumerate()
                .map(|(step_index, planned)| ActionNode::Operation {
                    kind: action_kind(&planned.step),
                    step_index,
                    requirement: planned.requirement,
                }),
        );
        let product_start = nodes.len();
        nodes.extend(
            reaction
                .products()
                .iter()
                .map(|species| ActionNode::Chemical {
                    composition: species.composition.clone(),
                    role: ChemicalRole::Product,
                }),
        );

        let mut edges = Vec::new();
        for precursor in 0..precursor_count {
            edges.push((precursor, operation_start));
        }
        for operation in 0..steps.len().saturating_sub(1) {
            edges.push((operation_start + operation, operation_start + operation + 1));
        }
        let last_operation = operation_start + steps.len() - 1;
        for product in 0..reaction.products().len() {
            edges.push((last_operation, product_start + product));
        }

        Ok(Self { nodes, edges })
    }

    /// Converts a plan while preserving the plan's existing validation and
    /// unresolved-step annotations.
    pub fn from_plan(plan: &SynthesisPlan) -> Result<Self, ActionGraphError> {
        let reaction = plan
            .balanced_reaction
            .as_ref()
            .ok_or(ActionGraphError::MissingBalancedReaction)?;
        Self::from_reaction_and_steps(reaction, &plan.steps)
    }

    pub fn nodes(&self) -> &[ActionNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[(usize, usize)] {
        &self.edges
    }

    /// Returns a row-major 0/1 adjacency matrix in node-index order.
    pub fn adjacency_matrix(&self) -> Vec<Vec<u8>> {
        let mut matrix = vec![vec![0; self.nodes.len()]; self.nodes.len()];
        for &(source, destination) in &self.edges {
            matrix[source][destination] = 1;
        }
        matrix
    }

    pub fn operation_kinds(&self) -> Vec<ActionKind> {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                ActionNode::Operation { kind, .. } => Some(*kind),
                ActionNode::Chemical { .. } => None,
            })
            .collect()
    }
}

fn action_kind(step: &ProcessStep) -> ActionKind {
    match step {
        ProcessStep::Weigh { .. } => ActionKind::StartingSynthesis,
        ProcessStep::Mix { .. } => ActionKind::MixingOperation,
        ProcessStep::Grind { .. } => ActionKind::MillingOperation,
        ProcessStep::Form { .. } => ActionKind::ShapingOperation,
        ProcessStep::Heat { .. } => ActionKind::HeatingOperation,
        ProcessStep::Cool { .. } => ActionKind::QuenchingOperation,
        ProcessStep::IntermediateCharacterization { .. } => ActionKind::CharacterizationOperation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{MixingMethod, PlannedStep};
    use crate::{Composition, Element, ReactionSpecies};

    fn c(element: &str) -> Composition {
        Composition::new([(Element::new(element).unwrap(), 1.0)]).unwrap()
    }

    #[test]
    fn preserves_plan_order_and_chemical_operation_edges() {
        let reaction = BalancedReaction::new(
            vec![ReactionSpecies::new(c("Fe"), 1).unwrap()],
            vec![ReactionSpecies::new(c("Fe"), 1).unwrap()],
        )
        .unwrap();
        let steps = vec![PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Mix {
                method: MixingMethod::DryMixing,
            },
        }];
        let graph = ActionGraph::from_reaction_and_steps(&reaction, &steps).unwrap();
        assert_eq!(graph.operation_kinds(), vec![ActionKind::MixingOperation]);
        assert_eq!(graph.edges(), &[(0, 1), (1, 2)]);
        assert_eq!(graph.adjacency_matrix()[0][1], 1);
        assert_eq!(graph.adjacency_matrix()[1][2], 1);
    }

    #[test]
    fn refuses_empty_process() {
        let reaction = BalancedReaction::new(
            vec![ReactionSpecies::new(c("Fe"), 1).unwrap()],
            vec![ReactionSpecies::new(c("Fe"), 1).unwrap()],
        )
        .unwrap();
        assert_eq!(
            ActionGraph::from_reaction_and_steps(&reaction, &[]),
            Err(ActionGraphError::EmptyProcess)
        );
    }
}
