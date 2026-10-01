const invoke = window.__TAURI__.core.invoke;
let selectedId = null;

function showError(message) {
  const el = document.getElementById("error");
  el.hidden = !message;
  el.textContent = message || "";
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args || {});
  } catch (err) {
    const message = typeof err === "string" ? err : (err && err.message) || String(err);
    showError(message);
    throw err;
  }
}

function renderCases(cases) {
  const list = document.getElementById("cases");
  list.replaceChildren();
  if (!cases.length) {
    const empty = document.createElement("li");
    empty.textContent = "No cases.";
    empty.style.cursor = "default";
    list.appendChild(empty);
    return;
  }
  for (const item of cases) {
    const li = document.createElement("li");
    li.dataset.id = String(item.id);
    if (item.id === selectedId) li.className = "active";
    const num = document.createElement("span");
    num.className = "num";
    num.textContent = item.case_number + " · " + item.status;
    const meta = document.createElement("span");
    meta.className = "meta";
    meta.textContent = item.defendant_name;
    li.append(num, meta);
    list.appendChild(li);
  }
}

async function loadBoard() {
  showError("");
  const [cases, lines, path] = await Promise.all([
    call("list_cases"),
    call("reminders", { days: 3 }),
    call("db_path"),
  ]);
  renderCases(cases);
  document.getElementById("db-path").textContent = path;
  const reminders = document.getElementById("reminders");
  reminders.replaceChildren();
  if (!lines.length) {
    const quiet = document.createElement("li");
    quiet.textContent = "Board is clear.";
    quiet.style.cursor = "default";
    reminders.appendChild(quiet);
    return;
  }
  for (const line of lines) {
    const li = document.createElement("li");
    li.textContent = line;
    reminders.appendChild(li);
  }
}

function field(form, name) {
  const value = form.elements[name].value.trim();
  return value || null;
}

async function openCase(id) {
  selectedId = id;
  const view = await call("case_view", { id });
  const root = document.getElementById("detail");
  root.replaceChildren();

  const title = document.createElement("h2");
  title.textContent = view.case.case_number;
  const who = document.createElement("p");
  who.textContent = view.case.defendant_name + " · " + view.case.status;
  const charges = document.createElement("p");
  charges.textContent = view.case.charges;

  const actions = document.createElement("div");
  actions.className = "actions";
  const pdf = document.createElement("button");
  pdf.className = "btn";
  pdf.type = "button";
  pdf.dataset.action = "pdf";
  pdf.dataset.id = String(id);
  pdf.textContent = "Save PDF";
  const close = document.createElement("button");
  close.className = "btn danger";
  close.type = "button";
  close.dataset.action = "close";
  close.dataset.id = String(id);
  close.textContent = "Close case";
  actions.append(pdf, close);

  const saved = document.createElement("p");
  saved.id = "saved";
  saved.className = "saved";

  const summary = document.createElement("pre");
  summary.className = "summary";
  summary.textContent = view.summary;

  const form = document.createElement("form");
  form.id = "new-deadline";
  form.dataset.caseId = String(id);
  form.innerHTML = "<h2>Deadline</h2>";
  const kind = document.createElement("input");
  kind.name = "kind";
  kind.required = true;
  kind.placeholder = "Kind";
  const due = document.createElement("input");
  due.name = "due_date";
  due.type = "date";
  due.required = true;
  const notes = document.createElement("input");
  notes.name = "notes";
  notes.placeholder = "Notes";
  const add = document.createElement("button");
  add.className = "btn";
  add.type = "submit";
  add.textContent = "Add deadline";
  form.append(kind, due, notes, add);

  root.append(title, who, charges, actions, saved, summary, form);
  document.querySelectorAll("#cases li").forEach((li) => {
    li.classList.toggle("active", Number(li.dataset.id) === id);
  });
}

document.addEventListener("click", async (event) => {
  const caseRow = event.target.closest("#cases li[data-id]");
  if (caseRow) {
    await openCase(Number(caseRow.dataset.id));
    return;
  }
  const button = event.target.closest("[data-action]");
  if (!button) return;
  const id = Number(button.dataset.id);
  if (button.dataset.action === "pdf") {
    const path = await call("save_pdf", { id });
    document.getElementById("saved").textContent = "Wrote " + path;
  }
  if (button.dataset.action === "close") {
    await call("close_case", { id });
    await loadBoard();
    await openCase(id);
  }
});

document.addEventListener("submit", async (event) => {
  if (event.target.matches("#new-case")) {
    event.preventDefault();
    const form = event.target;
    const created = await call("add_case", {
      caseNumber: field(form, "case_number"),
      pseudonym: field(form, "pseudonym"),
      charges: field(form, "charges"),
      court: field(form, "court"),
      notes: field(form, "notes"),
    });
    form.reset();
    await loadBoard();
    await openCase(created.id);
  }
  if (event.target.matches("#new-deadline")) {
    event.preventDefault();
    const form = event.target;
    await call("add_deadline", {
      caseId: Number(form.dataset.caseId),
      kind: field(form, "kind"),
      dueDate: field(form, "due_date"),
      notes: field(form, "notes"),
    });
    await openCase(Number(form.dataset.caseId));
    await loadBoard();
  }
});

document.getElementById("search-btn").addEventListener("click", async () => {
  const term = document.getElementById("search").value.trim();
  if (!term) return;
  renderCases(await call("search_cases", { term }));
});

document.getElementById("clear-search").addEventListener("click", loadBoard);

loadBoard();
