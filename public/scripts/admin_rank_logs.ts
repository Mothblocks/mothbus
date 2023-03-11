import { createPaginatedPage } from "./paginated";

const adminRankLogs = document.getElementById("admin_rank_logs")!;
const operationsList = document.getElementById("operations_list")!;

const OPERATIONS = [
  "add admin",
  "remove admin",
  "change admin rank",
  "add rank",
  "remove rank",
  "change rank flags",
];

createPaginatedPage(async (page) => {
  const response = await fetch(
    `${location.origin}${location.pathname}?page=${page}&embed`
  );

  if (!response.ok) {
    alert("Couldn't get new admin rank logs!");
    return;
  }

  return response.text();
}, adminRankLogs);

const extraStyle = document.createElement("style");
document.head.appendChild(extraStyle);

const operationsToggled = new Set(OPERATIONS);

for (const [index, operation] of Object.entries(OPERATIONS)) {
  const operationOption = document.createElement("span");
  operationOption.className = "operation-option";

  const checkbox = document.createElement("input");
  checkbox.id = `operation_${index}`;
  checkbox.type = "checkbox";
  checkbox.checked = true;

  checkbox.addEventListener("change", () => {
    if (checkbox.checked) {
      operationsToggled.add(operation);
    } else {
      operationsToggled.delete(operation);
    }

    extraStyle.innerHTML = "";

    for (const toggledOperation of OPERATIONS) {
      if (operationsToggled.has(toggledOperation)) {
        continue;
      }

      extraStyle.innerHTML += `.rank-log[data-operation="${toggledOperation}"] { display: none; }`;
    }
  });

  const label = document.createElement("label");
  label.htmlFor = `operation_${index}`;
  label.innerText = operation;

  operationOption.appendChild(checkbox);
  operationOption.appendChild(label);

  operationsList.appendChild(operationOption);
}
