import { createPaginatedPage } from "./paginated";

const tickets_list = document.getElementById("tickets_list")!;

tickets_list.addEventListener("click", (event) => {
  const target = event.target as HTMLAnchorElement;
  if (target.tagName !== "A") {
    return;
  }

  const ticket = target.closest(".ticket");
  if (ticket === null) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  const pageAttribute = ticket.getAttribute("data-page");
  const pageSearch = pageAttribute ? `?page=${pageAttribute}` : "";

  history.replaceState(
    "",
    "",
    `${location.pathname}${pageSearch}#${ticket.id}`
  );

  location.assign(target.href);
});

createPaginatedPage(async (page) => {
  const response = await fetch(
    `${location.origin}${location.pathname}?page=${page}&embed`
  );

  if (!response.ok) {
    alert("Couldn't get new tickets!");
    return;
  }

  return response.text();
}, tickets_list);
