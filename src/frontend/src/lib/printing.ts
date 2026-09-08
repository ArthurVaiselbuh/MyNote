import { renderBody } from "./markdown";

let preparingPrint = false;

export async function printNote(content: { title: string; body: string }) {
  if (preparingPrint) return;
  preparingPrint = true;
  const article = document.createElement("article");
  article.className = "print-note preview";
  const heading = document.createElement("h1");
  heading.textContent = content.title;
  const body = document.createElement("div");
  body.innerHTML = renderBody(content.body);
  article.append(heading, body);
  document.body.append(article);
  const finishPrint = () => {
    window.removeEventListener("afterprint", finishPrint);
    article.remove();
    preparingPrint = false;
  };
  try {
    await Promise.all(Array.from(article.querySelectorAll("img"), image => image.decode().catch(() => {})));
    window.addEventListener("afterprint", finishPrint);
    window.print();
  } catch (error) {
    finishPrint();
    throw error;
  }
}
