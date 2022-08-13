const page_counter = document.getElementById("page_counter")!;
const tickets_list = document.getElementById("tickets_list")!;

const urlParams = new URLSearchParams(window.location.search);

let retrieving = false;
let page = parseInt(urlParams.get("page") || "1", 10) || 1;

const updatePageText = () => {
  page_counter.innerText = `page ${page} - `;
};

updatePageText();

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

const backToPage1Link = document.getElementById(
  "back_to_page_1_link"
) as HTMLAnchorElement;

backToPage1Link.href = location.pathname;

window.addEventListener("scroll", () => {
  const { scrollHeight, clientHeight, scrollTop } = document.documentElement;

  if (scrollTop + clientHeight >= scrollHeight - 100 && !retrieving) {
    retrieving = true;
    page += 1;

    fetch(`${location.origin}${location.pathname}?page=${page}&embed`)
      .then(async (response) => {
        if (response.ok) {
          tickets_list.innerHTML += await response.text();

          window.history.replaceState(
            "",
            "",
            `${location.pathname}?page=${page}${location.hash}`
          );
          updatePageText();
        } else {
          alert("Couldn't get new tickets!");
        }
      })
      .finally(() => {
        retrieving = false;
      });
  }
});
