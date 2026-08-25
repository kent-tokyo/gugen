// Pure rendering functions over a parsed SynthesisPlanningReport (the
// exact JSON gugen::Planner::plan already produces -- no new backend
// concepts invented here, only presentation). Builds DOM nodes directly
// (no innerHTML with interpolated data anywhere) -- all report text is
// set via textContent, so this stays safe even once free-form input
// (stage 2) can put arbitrary strings into a report's fields.

function el(tag, opts = {}, children = []) {
  const node = document.createElement(tag);
  if (opts.className) node.className = opts.className;
  if (opts.text !== undefined) node.textContent = opts.text;
  for (const child of children) {
    if (child) node.appendChild(child);
  }
  return node;
}

function formatComposition(composition) {
  return Object.entries(composition)
    .map(([symbol, amount]) => `${symbol}:${amount}`)
    .join(" ");
}

function formatReaction(reaction) {
  const side = (species) =>
    species
      .map((s) => `${s.coefficient}×(${formatComposition(s.composition)})`)
      .join(" + ");
  return `${side(reaction.reactants)} → ${side(reaction.products)}`;
}

// ProcessStep is a Rust enum serialized as { "Variant": { ...fields } }.
// Renders every field generically -- an unset (null) field is shown as
// an explicit "unresolved" label, never left blank, matching gugen's own
// "abstention, not a guess" convention.
function formatStepField(key, value) {
  if (value === null || value === undefined) {
    return el("span", { className: "unresolved", text: `${key}: unresolved` });
  }
  if (Array.isArray(value)) {
    const list = el("ul", { className: "materials" });
    for (const item of value) {
      if (typeof item === "object" && item !== null) {
        list.appendChild(
          el("li", {
            text: `${item.precursor ?? ""} ×${item.formula_units ?? "?"}`,
          })
        );
      } else {
        list.appendChild(el("li", { text: String(item) }));
      }
    }
    return el("div", {}, [el("span", { text: `${key}:` }), list]);
  }
  return el("span", { text: `${key}: ${value}` });
}

function renderStep(plannedStep) {
  const variant = Object.keys(plannedStep.step)[0];
  const fields = plannedStep.step[variant];
  const row = el("div", { className: `step step-${plannedStep.requirement.toLowerCase()}` });
  row.appendChild(
    el("span", { className: "step-requirement", text: plannedStep.requirement })
  );
  row.appendChild(el("span", { className: "step-name", text: variant }));
  const fieldList = el("div", { className: "step-fields" });
  for (const [key, value] of Object.entries(fields)) {
    fieldList.appendChild(formatStepField(key, value));
  }
  row.appendChild(fieldList);
  return row;
}

function renderPlanCard(plan) {
  const card = el("div", { className: "plan-card" });
  card.appendChild(
    el("h3", { text: `${plan.route_family} — score ${plan.score.total_ranking_score.toFixed(4)}` })
  );
  if (plan.balanced_reaction) {
    card.appendChild(el("p", { className: "reaction", text: formatReaction(plan.balanced_reaction) }));
  }
  card.appendChild(
    el("p", {
      className: "precursors",
      text: "Precursors: " + plan.precursors.map((p) => `${p.precursor} ×${p.formula_units}`).join(", "),
    })
  );

  card.appendChild(el("h4", { text: "Process steps" }));
  const steps = el("div", { className: "steps" });
  for (const step of plan.steps) steps.appendChild(renderStep(step));
  card.appendChild(steps);

  if (plan.evidence.length > 0) {
    card.appendChild(el("h4", { text: "Evidence" }));
    const list = el("ul", { className: "evidence" });
    for (const e of plan.evidence) {
      list.appendChild(el("li", { text: `[${e.strength}/${e.kind}] ${e.statement}` }));
    }
    card.appendChild(list);
  }

  if (plan.assumptions.length > 0) {
    card.appendChild(el("h4", { text: "Assumptions" }));
    const list = el("ul", { className: "assumptions" });
    for (const a of plan.assumptions) list.appendChild(el("li", { text: a.statement }));
    card.appendChild(list);
  }

  if (plan.unresolved.length > 0) {
    card.appendChild(el("h4", { text: "Unresolved conditions" }));
    const list = el("ul", { className: "unresolved-list" });
    for (const u of plan.unresolved) {
      list.appendChild(el("li", { text: `${u.description} — ${u.reason}` }));
    }
    card.appendChild(list);
  }

  if (plan.warnings.length > 0) {
    card.appendChild(el("h4", { text: "Warnings" }));
    const list = el("ul", { className: "warnings" });
    for (const w of plan.warnings) {
      list.appendChild(el("li", { className: `severity-${w.severity.toLowerCase()}`, text: w.message }));
    }
    card.appendChild(list);
  }

  if (plan.manual_review_required) {
    card.appendChild(el("p", { className: "manual-review", text: "Manual review required before use." }));
  }

  return card;
}

export function renderAccepted(report, container) {
  container.replaceChildren();
  if (report.plans.length === 0) {
    container.appendChild(
      el("p", { className: "empty", text: "No accepted plans for this target/catalog." })
    );
    return;
  }
  for (const plan of report.plans) container.appendChild(renderPlanCard(plan));
}

export function renderRejected(report, container) {
  container.replaceChildren();
  if (report.rejected_candidates.length === 0) {
    container.appendChild(
      el("p", { className: "empty", text: "No rejected candidate combinations were recorded." })
    );
    return;
  }
  for (const rejection of report.rejected_candidates) {
    const card = el("div", { className: "rejection-card" });
    card.appendChild(
      el("h3", { text: rejection.precursors.length > 0 ? rejection.precursors.join(" + ") : "(no candidates)" })
    );
    card.appendChild(
      el("p", { className: "reason-codes", text: rejection.reason_codes.join(", ") })
    );
    card.appendChild(el("p", { className: "explanation", text: rejection.explanation }));
    container.appendChild(card);
  }
}

function toMarkdown(report) {
  const lines = [`# Synthesis Planning Report (schema v${report.schema_version})`, ""];
  lines.push(`**Target:** ${formatComposition(report.target.composition)}`, "");
  lines.push(`**Applicability:** ${report.applicability.level}`, "");
  for (const plan of report.plans) {
    lines.push(`## Plan ${plan.plan_id} (score ${plan.score.total_ranking_score.toFixed(4)})`, "");
    lines.push(`- Route family: ${plan.route_family}`);
    if (plan.balanced_reaction) lines.push(`- Reaction: ${formatReaction(plan.balanced_reaction)}`);
    lines.push(`- Manual review required: ${plan.manual_review_required}`, "");
  }
  if (report.rejected_candidates.length > 0) {
    lines.push("## Rejected candidates", "");
    for (const r of report.rejected_candidates) {
      lines.push(`- ${r.precursors.join(" + ") || "(none)"}: ${r.reason_codes.join(", ")} — ${r.explanation}`);
    }
  }
  return lines.join("\n");
}

export function renderJsonAndMarkdown(report, jsonContainer, markdownContainer) {
  jsonContainer.textContent = JSON.stringify(report, null, 2);
  markdownContainer.textContent = toMarkdown(report);
}
