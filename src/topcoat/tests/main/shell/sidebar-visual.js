const { strict: assert } = require("node:assert");
const { resolve } = require("node:path");
const accents = ["indigo", "teal", "rose", "amber", "green", "violet"];
async function appearance(page, theme, accent = "indigo", scale = "md", compact = false) {
  await page.evaluate(({ theme, accent, scale, compact }) => {
    const html = document.documentElement;
    html.dataset.theme = theme;
    html.classList.toggle("dark", theme === "dark");
    html.classList.toggle("density-compact", compact);
    html.dataset.accent = accent;
    html.dataset.fontScale = { sm: "small", md: "normal", lg: "large" }[scale];
    html.dataset.density = compact ? "compact" : "comfortable";
  }, { theme, accent, scale, compact });
}
async function sidebarContrast(page, selector = "aside.tc-shell__desktop") {
  return page.locator(selector).evaluate((root) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    function rgb(color) {
      ctx.clearRect(0, 0, 1, 1);
      ctx.fillStyle = color;
      ctx.fillRect(0, 0, 1, 1);
      return [...ctx.getImageData(0, 0, 1, 1).data];
    }
    function background(el) {
      if (!el)
        return [255, 255, 255, 255];
      const fg = rgb(getComputedStyle(el).backgroundColor);
      if (fg[3] === 255)
        return fg;
      const bg = background(el.parentElement), alpha = fg[3] / 255;
      return fg.slice(0, 3).map((v, i) => v * alpha + bg[i] * (1 - alpha)).concat(255);
    }
    function ratio(a, b) {
      const luminance = (c) => c.slice(0, 3).map((v) => {
        v /= 255;
        return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
      }).reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
      const l1 = luminance(a), l2 = luminance(b);
      return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    }
    const rows = [...root.querySelectorAll("*")].filter((el) => el.getClientRects().length && !el.closest("[hidden], [inert]") && [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent?.trim())).map((el) => ({
      label: el.textContent.trim().slice(0, 60),
      ratio: ratio(rgb(getComputedStyle(el).color), background(el))
    }));
    const selected = root.querySelector('.tc-shell__navigation a[aria-current="page"]') ?? root.querySelector('a[aria-current="page"]');
    const avatar = root.querySelector("[data-lific-account-name]");
    const probe = document.createElement("span");
    probe.style.color = "var(--tc-accent)";
    root.append(probe);
    const accent = rgb(getComputedStyle(probe).color);
    probe.style.color = "var(--tc-focus)";
    const focus = rgb(getComputedStyle(probe).color);
    probe.remove();
    return {
      minimumText: Math.min(...rows.map((row) => row.ratio)),
      weakestText: rows.reduce((a, b) => a.ratio < b.ratio ? a : b),
      selectedText: selected ? ratio(rgb(getComputedStyle(selected).color), background(selected)) : null,
      avatarText: avatar ? ratio(rgb(getComputedStyle(avatar).color), background(avatar)) : null,
      marker: ratio(accent, background(root)),
      focusOnSelection: ratio(focus, background(selected ?? root)),
      selectedFill: selected ? rgb(getComputedStyle(selected).backgroundColor) : null
    };
  });
}
async function checkSidebarContrast(page, directory) {
  const results = [];
  const selected = page.locator('aside a[href$="/ONE/issues"][aria-current="page"]');
  const neutral = page.locator('aside a[href$="/ONE/board"]');
  for (const theme of ["light", "dark"])
    for (const accent of accents) {
      await appearance(page, theme, accent);
      await neutral.hover();
      await page.waitForTimeout(180);
      const result = { theme, accent, ...await sidebarContrast(page) };
      assert.ok(result.minimumText >= 4.5, JSON.stringify(result));
      assert.ok(result.marker >= 3 && result.focusOnSelection >= 3, JSON.stringify(result));
      assert.notEqual(await selected.evaluate((el) => getComputedStyle(el).backgroundColor), await neutral.evaluate((el) => getComputedStyle(el).backgroundColor), "Selection differs from hover");
      assert.equal(await page.locator('aside [data-project-id="1"]').evaluate((el) => getComputedStyle(el.parentElement).backgroundColor), "rgba(0, 0, 0, 0)", "Expanded active parent stays transparent");
      results.push(result);
    }
  await require("node:fs/promises").writeFile(resolve(directory, "sidebar-contrast.json"), JSON.stringify(results, null, 2));
  console.table(results.map(({ theme, accent, minimumText, selectedText, avatarText, marker }) => ({ theme, accent, minimumText: minimumText.toFixed(2), selectedText: selectedText?.toFixed(2), avatarText: avatarText?.toFixed(2), marker: marker.toFixed(2) })));
}
async function loadVisualFonts(page) {
  await page.evaluate(async () => {
    await document.fonts.ready;
    const font = getComputedStyle(document.body).font;
    await document.fonts.load(font);
    if (!document.fonts.check(font))
      throw Error("Visual fonts did not load");
  });
}
async function desktopScreenshots(page, directory) {
  await loadVisualFonts(page);
  await page.locator("aside [data-recents-toggle]").click();
  for (const theme of ["light", "dark"]) {
    for (const width of [190, 230, 360]) {
      const handle = page.getByRole("separator", { name: "Resize sidebar" });
      await handle.dblclick();
      await handle.focus();
      const steps = Math.abs(width - 230) / 10;
      for (let i = 0;i < steps; i++)
        await page.keyboard.press(width > 230 ? "ArrowRight" : "ArrowLeft");
      for (const scale of ["sm", "md", "lg"])
        for (const compact of [false, true]) {
          await appearance(page, theme, "indigo", scale, compact);
          await page.mouse.move(900, 100);
          await page.locator("aside.tc-shell__desktop").evaluate((el) => el.scrollTop = 0);
          await page.locator("main").click();
          await page.screenshot({ path: resolve(directory, `desktop-${theme}-${width}-${scale}-${compact ? "compact" : "comfortable"}.png`) });
          assert.ok(await page.locator("aside.tc-shell__desktop").evaluate((el) => el.scrollWidth <= el.clientWidth), "Sidebar must not scroll horizontally");
        }
      await appearance(page, theme);
    }
  }
  await page.locator('aside button[aria-label="Expand Two"]').click();
  await page.locator('aside button[aria-label="Expand Four"]').click();
  for (const theme of ["light", "dark"]) {
    await appearance(page, theme);
    for (const position of ["top", "bottom"]) {
      await page.locator("aside.tc-shell__desktop").evaluate((el, position) => el.scrollTop = position === "top" ? 0 : el.scrollHeight, position);
      await page.screenshot({ path: resolve(directory, `desktop-multiple-expanded-${theme}-${position}.png`) });
    }
  }
}
async function touchGeometry(page, selector) {
  const failures = await page.locator(selector).evaluate((root) => [...root.querySelectorAll("a, button")].filter((el) => el.getClientRects().length && !el.closest("[inert], [hidden]")).flatMap((el) => {
    const b = el.getBoundingClientRect();
    const siblings = [...el.parentElement.children].filter((sibling) => sibling !== el && sibling.matches("a, button"));
    const overlap = siblings.some((sibling) => {
      const other = sibling.getBoundingClientRect();
      return Math.min(b.right, other.right) - Math.max(b.left, other.left) > 1 && Math.min(b.bottom, other.bottom) - Math.max(b.top, other.top) > 1;
    });
    return b.width < 44 || b.height < 44 || b.x < 0 || b.right > innerWidth || overlap ? [{ label: el.getAttribute("aria-label") ?? el.textContent, x: b.x, width: b.width, height: b.height }] : [];
  }));
  assert.deepEqual(failures, [], "Every mobile control retains a non-overlapping, in-bounds 44px touch target");
  const mainRows = page.locator(`${selector} [data-mobile-project-trigger], ${selector} nav [data-mobile-destination]`);
  assert.ok(await mainRows.evaluateAll((rows) => rows.every((el) => el.getBoundingClientRect().height >= 48)), "Main navigation keeps 48px or larger rows at every text size");
  assert.ok(await page.locator(`${selector} nav`).evaluate((el) => el.scrollWidth <= el.clientWidth), "No horizontal mobile navigation scroll");
}
async function mobileVisualChecks(page, directory, screenshots) {
  if (screenshots)
    await loadVisualFonts(page);
  const root = page.locator("[data-mobile-root]");
  const project = page.locator("[data-mobile-project]");
  const depth = (n) => page.waitForFunction((n) => history.state?.lificMobileNav?.depth === n, n);
  await page.getByRole("button", { name: "Open navigation", exact: true }).click();
  await depth(1);
  const results = [];
  for (const theme of ["light", "dark"])
    for (const accent of accents) {
      await appearance(page, theme, accent);
      await page.waitForFunction(() => new DOMMatrix(getComputedStyle(document.querySelector("[data-mobile-navigation]")).transform).m41 === 0);
      const rootResult = await sidebarContrast(page, "[data-mobile-root]");
      assert.ok(rootResult.minimumText >= 4.5, JSON.stringify({ theme, accent, rootResult }));
      await root.getByRole("button", { name: "Open One navigation", exact: true }).click();
      await depth(2);
      const projectResult = await sidebarContrast(page, "[data-mobile-project]");
      assert.ok(projectResult.minimumText >= 4.5 && projectResult.focusOnSelection >= 3, JSON.stringify({ theme, accent, projectResult }));
      results.push({ theme, accent, root: rootResult, project: projectResult });
      await page.keyboard.press("Escape");
      await depth(1);
    }
  await require("node:fs/promises").writeFile(resolve(directory, "mobile-sidebar-contrast.json"), JSON.stringify(results, null, 2));
  for (const theme of ["light", "dark"])
    for (const scale of ["sm", "md", "lg"])
      for (const compact of [false, true]) {
        await appearance(page, theme, "indigo", scale, compact);
        await touchGeometry(page, "[data-mobile-root]");
        const suffix = `${theme}-${scale}-${compact ? "compact" : "comfortable"}`;
        if (screenshots) {
          await root.locator(".tc-mobile__brand").click();
          await page.screenshot({ path: resolve(directory, `mobile-root-${suffix}.png`) });
        }
        await root.getByRole("button", { name: "Open One navigation", exact: true }).click();
        await depth(2);
        await touchGeometry(page, "[data-mobile-project]");
        const selected = project.locator('nav a[aria-current="page"]');
        assert.equal(await selected.count(), 1, "Exactly one selected destination");
        if (screenshots) {
          await project.locator("h2").click();
          await page.screenshot({ path: resolve(directory, `mobile-project-${suffix}.png`) });
        }
        await page.keyboard.press("Escape");
        await depth(1);
      }
}
module.exports = { accents, appearance, sidebarContrast, checkSidebarContrast, loadVisualFonts, desktopScreenshots, mobileVisualChecks, touchGeometry };
