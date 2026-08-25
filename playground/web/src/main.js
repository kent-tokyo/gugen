import init, { plan_synthesis } from "../pkg/gugen_playground_wasm.js";
import { EXAMPLES } from "./examples.js";
import { renderAccepted, renderRejected, renderJsonAndMarkdown } from "./render.js";
import { initCustomTarget, readCustomRequest } from "./custom.js";

const state = { selected: EXAMPLES[0], report: null };

function renderExampleList() {
  const list = document.getElementById("example-list");
  list.replaceChildren();
  for (const example of EXAMPLES) {
    const isSelected = example.id === state.selected.id;
    const card = document.createElement("button");
    card.className = "example-card" + (isSelected ? " selected" : "");
    card.type = "button";
    card.setAttribute("aria-pressed", isSelected ? "true" : "false");
    card.setAttribute("aria-label", `${example.name}: ${example.what}`);

    const title = document.createElement("h3");
    title.textContent = example.name;
    card.appendChild(title);

    const category = document.createElement("p");
    category.className = "category";
    category.textContent = example.category;
    card.appendChild(category);

    const what = document.createElement("p");
    what.textContent = example.what;
    card.appendChild(what);

    const lookFor = document.createElement("p");
    lookFor.className = "look-for";
    lookFor.textContent = "Look for: " + example.look_for;
    card.appendChild(lookFor);

    const unresolved = document.createElement("p");
    unresolved.className = "unresolved-note";
    unresolved.textContent = "Stays unresolved: " + example.unresolved;
    card.appendChild(unresolved);

    card.addEventListener("click", () => {
      state.selected = example;
      renderExampleList();
      renderCitation();
      hideResults();
    });
    list.appendChild(card);
  }
}

function renderCitation() {
  document.getElementById("citation").textContent = state.selected.citation;
}

function hideResults() {
  document.getElementById("results").hidden = true;
  document.getElementById("status").textContent = "";
}

function showTab(tabName, options = {}) {
  for (const panel of document.querySelectorAll(".tab-panel")) {
    panel.hidden = panel.dataset.tab !== tabName;
  }
  for (const button of document.querySelectorAll(".tab-button")) {
    const isSelected = button.dataset.tab === tabName;
    button.classList.toggle("active", isSelected);
    button.setAttribute("aria-selected", isSelected ? "true" : "false");
    button.tabIndex = isSelected ? 0 : -1;
    if (isSelected && options.focus) {
      button.focus();
    }
  }
}

// WAI-ARIA APG tabs pattern: Left/Right/Home/End move focus and activate
// (automatic activation) -- roving tabindex means only the selected tab is
// in the page's Tab order; arrow keys move between tabs directly.
function wireTabs() {
  const buttons = Array.from(document.querySelectorAll(".tab-button"));
  for (const button of buttons) {
    button.addEventListener("click", () => showTab(button.dataset.tab));
    button.addEventListener("keydown", (event) => {
      const index = buttons.indexOf(button);
      let target = null;
      if (event.key === "ArrowRight") {
        target = buttons[(index + 1) % buttons.length];
      } else if (event.key === "ArrowLeft") {
        target = buttons[(index - 1 + buttons.length) % buttons.length];
      } else if (event.key === "Home") {
        target = buttons[0];
      } else if (event.key === "End") {
        target = buttons[buttons.length - 1];
      }
      if (target) {
        event.preventDefault();
        showTab(target.dataset.tab, { focus: true });
      }
    });
  }
}

async function copyWithFeedback(sourceId, feedbackId) {
  const feedback = document.getElementById(feedbackId);
  // navigator.clipboard.writeText() can hang indefinitely rather than
  // reject (observed waiting on a permission prompt with no one to answer
  // it) -- race it against a timeout so the user always gets an answer.
  const write = navigator.clipboard.writeText(document.getElementById(sourceId).textContent);
  const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 3000));
  try {
    await Promise.race([write, timeout]);
    feedback.textContent = "Copied!";
  } catch {
    feedback.textContent = "Copy failed — select the text manually.";
  }
  setTimeout(() => {
    feedback.textContent = "";
  }, 3000);
}

function wireCopyButtons() {
  document.getElementById("copy-json").addEventListener("click", () => {
    copyWithFeedback("json-view", "copy-json-feedback");
  });
  document.getElementById("copy-markdown").addEventListener("click", () => {
    copyWithFeedback("markdown-view", "copy-markdown-feedback");
  });
}

function runPlan(target_elements, candidates) {
  const request = {
    target_elements,
    candidates,
    execution_timestamp: new Date().toISOString(),
  };

  document.getElementById("status").textContent = "Planning…";
  document.getElementById("results").hidden = true;

  // Runs synchronously in this browser tab -- no network call, no
  // worker for the MVP; every example here completes well under a
  // second against gugen's default search budget.
  const resultJson = plan_synthesis(JSON.stringify(request));
  const result = JSON.parse(resultJson);

  if (result.error) {
    document.getElementById("status").textContent = `Error: ${result.error}`;
    return;
  }

  state.report = result;
  document.getElementById("status").textContent =
    `Plan generated: ${result.plans.length} accepted plan(s), ` +
    `${result.rejected_candidates.length} rejected candidate(s).`;
  document.getElementById("results").hidden = false;
  renderAccepted(result, document.getElementById("accepted-panel"));
  renderRejected(result, document.getElementById("rejected-panel"));
  renderJsonAndMarkdown(
    result,
    document.getElementById("json-view"),
    document.getElementById("markdown-view")
  );
  showTab("accepted");
}

function runExamplePlan() {
  runPlan(state.selected.target_elements, state.selected.candidates);
}

function runCustomPlan() {
  let request;
  try {
    request = readCustomRequest();
  } catch (error) {
    document.getElementById("status").textContent = `Error: ${error.message}`;
    document.getElementById("results").hidden = true;
    return;
  }
  runPlan(request.target_elements, request.candidates);
}

async function main() {
  await init();
  renderExampleList();
  renderCitation();
  wireTabs();
  wireCopyButtons();
  initCustomTarget();
  document.getElementById("generate").addEventListener("click", runExamplePlan);
  document.getElementById("generate-custom").addEventListener("click", runCustomPlan);
}

main();
