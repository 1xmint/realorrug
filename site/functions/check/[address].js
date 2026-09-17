// SPDX-License-Identifier: Apache-2.0
// Cloudflare Pages Function for `/check/:address`.
//
// Design 0025 §7: a shared check link should unfurl on X as that token's
// verdict card. X's crawler runs no JavaScript, so the SPA's per-page meta
// never reaches it. This rewrites the static index.html's preview tags on the
// way out, for every visitor alike, and the SPA still boots from the same
// page. The alternative, routing by user agent to a server-rendered shell,
// breaks whenever a crawler changes its name and serves different pages to
// different readers.
//
// The address is only ever placed into a URL after the shape check below; an
// input that is neither an EVM contract nor a base58 mint gets the site's
// default card, never a URL built from it.

const CARD_ORIGIN = "https://radar.heyvera.org";
const EVM = /^0x[0-9a-fA-F]{40}$/;
const BASE58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;

class SetContent {
  constructor(value) {
    this.value = value;
  }
  element(el) {
    el.setAttribute("content", this.value);
  }
}

export async function onRequestGet({ request, params, next }) {
  const page = await next();
  const address = String(params.address ?? "");
  if (!EVM.test(address) && !BASE58.test(address)) return page;
  const type = page.headers.get("content-type") ?? "";
  if (!type.includes("text/html")) return page;

  const card = `${CARD_ORIGIN}/v1/check/${address}/card.png`;
  const url = new URL(request.url);
  const here = `${url.origin}/check/${address}`;
  const alt = "Real or Rug's verdict card for this token: what the chain shows, never a price.";
  return new HTMLRewriter()
    .on('meta[property="og:image"]', new SetContent(card))
    .on('meta[name="twitter:image"]', new SetContent(card))
    .on('meta[property="og:url"]', new SetContent(here))
    .on('meta[property="og:image:alt"]', new SetContent(alt))
    .transform(page);
}
