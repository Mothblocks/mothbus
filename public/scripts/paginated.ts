export function createPaginatedPage(
  getNextPageHtml: (page: number) => Promise<string | undefined>,
  pageContents: HTMLElement
) {
  const page_counter = document.getElementById("page_counter")!;

  const urlParams = new URLSearchParams(window.location.search);

  let retrieving = false;
  let hitLastPage = false;

  let page = parseInt(urlParams.get("page") || "1", 10) || 1;

  const updatePageText = () => {
    page_counter.innerText = `page ${page} - `;
  };

  updatePageText();

  const backToPage1Link = document.getElementById(
    "back_to_page_1_link"
  ) as HTMLAnchorElement;

  backToPage1Link.href = location.pathname;

  const nextPageLink = document.getElementById(
    "next_page_link"
  )! as HTMLAnchorElement;

  async function updateNextPage() {
    if (hitLastPage) {
      return;
    }

    page += 1;

    const html = await getNextPageHtml(page);

    if (html === undefined) {
      return;
    }

    if (html === "") {
      hitLastPage = true;
      nextPageLink.innerText = "none left";
      return;
    }

    pageContents.innerHTML += html;

    window.history.replaceState(
      "",
      "",
      `${location.pathname}?page=${page}${location.hash}`
    );

    updatePageText();
  }

  async function nextPage() {
    if (retrieving) {
      return;
    }

    retrieving = true;

    await updateNextPage().finally(() => {
      retrieving = false;
    });
  }

  nextPageLink.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    await nextPage();
  });

  window.addEventListener("scroll", async () => {
    const { scrollHeight, clientHeight, scrollTop } = document.documentElement;

    if (scrollTop + clientHeight >= scrollHeight - 100 && !retrieving) {
      nextPage();
    }
  });
}
