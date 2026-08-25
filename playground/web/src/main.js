import init, { plan_synthesis } from "../pkg/gugen_playground_wasm.js";
import { EXAMPLES } from "./examples.js";
import { renderAccepted, renderRejected, renderJsonAndMarkdown } from "./render.js";

const state = { selected: EXAMPLES[0], report: null };

function renderExampleList() {
  const list = document.getElementById("example-list");
  list.replaceChildren();
  for (const example of EXAMPLES) {
    const card = document.createElement("button");
    card.className = "example-card" + (example.id === state.selected.id ? " selected" : "");
    card.type = "button";

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

function showTab(tabName) {
  for (const panel of document.querySelectorAll(".tab-panel")) {
    panel.hidden = panel.dataset.tab !== tabName;
  }
  for (const button of document.querySelectorAll(".tab-button")) {
    button.classList.toggle("active", button.dataset.tab === tabName);
  }
}

function wireTabs() {
  for (const button of document.querySelectorAll(".tab-button")) {
    button.addEventListener("click", () => showTab(button.dataset.tab));
  }
}

function wireCopyButtons() {
  document.getElementById("copy-json").addEventListener("click", async () => {
    await navigator.clipboard.writeText(document.getElementById("json-view").textContent);
  });
  document.getElementById("copy-markdown").addEventListener("click", async () => {
    await navigator.clipboard.writeText(document.getElementById("markdown-view").textContent);
  });
}

function runPlan() {
  const example = state.selected;
  const request = {
    target_elements: example.target_elements,
    candidates: example.candidates,
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
  document.getElementById("status").textContent = "";
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

async function main() {
  await init();
  renderExampleList();
  renderCitation();
  wireTabs();
  wireCopyButtons();
  document.getElementById("generate").addEventListener("click", runPlan);
}

main();
