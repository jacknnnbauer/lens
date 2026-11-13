// 下载按钮直接指向最新版本的安装包 / 便携版：读 GitHub 的最新 Release。
// 读不到（离线、接口限流）时保持原链接：Releases 页面。
(() => {
  const els = document.querySelectorAll("[data-dl], [data-ver]");
  if (!els.length) return;
  const apply = (r) => {
    const ver = String(r.tag_name || "").replace(/^v/, "");
    const find = (re) => (r.assets || []).find((a) => re.test(a.name));
    const pick = { setup: find(/-setup\.exe$/i), portable: find(/-portable\.zip$/i) };
    document.querySelectorAll("[data-dl]").forEach((a) => {
      const f = pick[a.dataset.dl];
      if (f) {
        a.href = f.browser_download_url;
        const name = a.closest(".opt")?.querySelector(".file");
        if (name) name.textContent = f.name;
      }
    });
    if (ver) document.querySelectorAll("[data-ver]").forEach((e) => (e.textContent = ver));
  };
  let cached = null;
  try { cached = JSON.parse(sessionStorage.getItem("lens-latest") || "null"); } catch {}
  if (cached) return apply(cached);
  fetch("https://api.github.com/repos/jacknnnbauer/lens/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((r) => {
      const slim = { tag_name: r.tag_name, assets: (r.assets || []).map((a) => ({ name: a.name, browser_download_url: a.browser_download_url })) };
      try { sessionStorage.setItem("lens-latest", JSON.stringify(slim)); } catch {}
      apply(slim);
    })
    .catch(() => {});
})();
