function check(value, message) { if (!value) throw new Error(message); }
export async function testExperience(page, origin, audit) {
  await page.goto(origin);
  await page.locator('widget-card [data-widget-loaded]').first().waitFor();
  check(await page.getByRole('link', {name:'首页',exact:true}).count() === 0, 'home navigation button remains');
  await page.evaluate(() => { window.__widgetProof = 'stable'; });
  await page.locator('widget-card').first().getByRole('button').click();
  await page.locator('widget-card').first().getByText('已更新', {exact:true}).waitFor();
  check(await page.evaluate(() => window.__widgetProof) === 'stable', 'widget refresh reloaded document');
  for (const width of [360, 768, 1440]) {
    await page.setViewportSize({width,height:900}); await audit();
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `home overflows ${width}`);
    if (width < 1000) {
      const toggle = page.locator('[data-dock-toggle]');
      await toggle.click(); check(await toggle.getAttribute('aria-expanded') === 'true', 'dock did not expand');
      await audit();
      await toggle.press('Escape'); check(await toggle.getAttribute('aria-expanded') === 'false', 'Escape did not collapse dock');
    }
  }
  await page.emulateMedia({colorScheme:'dark'}); await audit();
  await page.screenshot({path:'/tmp/sloray-glass-desktop.png',fullPage:true});
  await page.setViewportSize({width:390,height:844});
  await page.screenshot({path:'/tmp/sloray-glass-mobile.png',fullPage:false});
  await page.getByRole('button',{name:'展开',exact:true}).click();
  await page.locator('[data-dock-toggle]').press('Tab');
  await page.screenshot({path:'/tmp/sloray-glass-mobile-expanded.png',fullPage:false});
  await page.emulateMedia({reducedMotion:'reduce'});
  check(await page.locator('.dock-tool').first().evaluate(el => getComputedStyle(el).transitionDuration) === '0s', 'dock still animates in reduced motion');
  await page.emulateMedia({colorScheme:'light',reducedMotion:'no-preference'});
  await page.setViewportSize({width:1280,height:720});
  await page.goto(origin + '/posts/browser-seed');
  await page.locator('reading-tools').waitFor();
  await page.getByLabel('正文字号').selectOption('1.25');
  await page.reload();
  check(await page.getByLabel('正文字号').inputValue() === '1.25', 'reading size lost');
  await page.getByRole('button',{name:'专注阅读',exact:true}).click();
  check(await page.locator('body').getAttribute('data-reading-focus') !== null, 'focus mode missing');
  await page.getByRole('button',{name:'专注阅读',exact:true}).click();
  await page.getByText('正文目录',{exact:true}).click();
  check(await page.getByRole('navigation',{name:'正文目录'}).getByRole('link').count() === 1, 'article headings not in TOC');
  await audit();
  await page.goto(origin);
  console.log('PASS Dock 展开/Escape、360/768/1440 布局/axe、减少动态效果、群岛独立刷新、字号持久化、专注阅读与目录');
}

export async function testIdentity(page, context, origin, audit) {
  await page.goto(origin + '/admin/settings');
  await page.getByLabel('站点名称',{exact:true}).fill('微光书房');
  await page.getByLabel('Favicon 图片地址',{exact:true}).fill('/favicon.svg');
  await page.getByRole('button',{name:'保存站点标识',exact:true}).click();
  await page.getByText('站点标识已保存，公开页面刷新后生效。',{exact:true}).waitFor();
  await audit();
  await page.goto(origin);
  check(await page.title() === '微光书房', 'site title not configured');
  check(await page.locator('link[rel=icon]').getAttribute('href') === '/favicon.svg', 'favicon not configured');
  check((await (await context.request.get(origin + '/rss.xml')).text()).includes('<title>微光书房</title>'), 'RSS title not configured');
  await page.goto(origin + '/admin/settings');
  await page.getByLabel('站点名称',{exact:true}).fill('静线数据库');
  await page.getByLabel('Favicon 图片地址',{exact:true}).fill('');
  await page.getByRole('button',{name:'保存站点标识',exact:true}).click();
  await page.getByText('站点标识已保存，公开页面刷新后生效。',{exact:true}).waitFor();
  console.log('PASS 设置页站点名称/Favicon 保存、页面标题/导航/RSS 同步与恢复默认');
}
