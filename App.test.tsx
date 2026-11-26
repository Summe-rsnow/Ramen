import { expect, open, test } from "nsh/test";

test("opens Ramen shell, About dialog, Settings dialog, and Syntax Help", async () => {
  const app = await open(".", { width: 1040, height: 680, platform: "windows" });
  expect(app.texts()).toContain("Ramen");
  expect(app.texts()).toContain("v1.2.0");

  await app.press("label:设置与快捷键");
  await app.waitFor("设置与服务");
  expect(app.texts()).toContain("全局召唤快捷键");
  expect(app.texts()).toContain("后台索引服务 (gozd v0.7.1)");
  await app.press("关闭");

  await app.press("label:搜索语法");
  await app.waitFor("goz 搜索语法速查");
  await app.press("关闭");

  await app.press("label:关于 Ramen");
  await app.waitFor("版本更新");
  expect(app.texts()).toContain("Summe-rsnow/Ramen");
  await app.press("关闭");
});
