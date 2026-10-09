function check(value, message) { if (!value) throw new Error(message); }
export async function testExperience(page, origin, audit) {
  await page.goto(origin);
  await page.locator('widget-card [data-widget-loaded]').first().waitFor();
  check(await page.getByRole('link', {name:'首页',exact:true}).count() === 0, 'home navigation button remains');
  await page.evaluate(() => { window.__widgetProof = 'stable'; });
  await page.locator('widget-card').first().getByRole('button').click();
  await page.getByText('已更新', {exact:true}).waitFor();
  check(await page.evaluate(() => window.__widgetProof) === 'stable', 'widget refresh reloaded document');
  check(await page.locator('.site-dock').innerText() === '', 'Dock contains visible text');
  check(await page.locator('body').evaluate(el => getComputedStyle(el).backgroundImage) === 'none', 'default background gradient remains');
  for (const width of [320, 360, 768, 1440, 3840]) {
    await page.setViewportSize({width,height:900}); await audit();
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `home overflows ${width}`);
    if (width < 1000) {
      const toggle = page.locator('[data-dock-toggle]');
      await toggle.click(); check(await toggle.getAttribute('aria-expanded') === 'true', 'dock did not expand');
      await audit();
      await toggle.press('Escape'); check(await toggle.getAttribute('aria-expanded') === 'false', 'Escape did not collapse dock');
    }
  }
  await page.setViewportSize({width:1440,height:900});
  await page.emulateMedia({colorScheme:'dark'}); await audit();
  await page.screenshot({path:'/tmp/sloray-glass-desktop.png',fullPage:true});
  await page.setViewportSize({width:390,height:844});
  await page.screenshot({path:'/tmp/sloray-glass-mobile.png',fullPage:false});
  await page.getByRole('button',{name:'展开导航',exact:true}).click();
  await page.locator('[data-dock-toggle]').press('Tab');
  await page.screenshot({path:'/tmp/sloray-glass-mobile-expanded.png',fullPage:false});
  await page.emulateMedia({reducedMotion:'reduce'});
  check(await page.locator('.dock-tool').first().evaluate(el => getComputedStyle(el).transitionDuration) === '0s', 'dock still animates in reduced motion');
  await page.emulateMedia({colorScheme:'light',reducedMotion:'no-preference'});
  await page.setViewportSize({width:1280,height:720});
  await page.goto(origin + '/posts/browser-seed');
  await page.locator('reading-progress').waitFor({state:'attached'});
  check(await page.locator('reading-tools,.reader-bar').count() === 0, 'removed reader controls remain');
  check(await page.locator('.reading-summary').textContent() === '在文字与交互之间，留出阅读的空间。', 'summary missing');
  check(await page.locator('.discussion button,.discussion form').count() === 0, 'article still contains actions');
  await testDockActions(page, origin, audit);
  check(await page.getByRole('navigation',{name:'正文目录'}).getByRole('link').count() === 1, 'article headings not in TOC');
  await audit();
  await testReadingToc(page, origin, audit);
  await testDockFallback(page.context().browser(), origin);
  await page.setViewportSize({width:1280,height:720});
  await page.goto(origin);
  console.log('PASS 纯图标 Dock 展开/Escape、360/768/1440/3840 布局/axe、减少动态效果、群岛独立刷新、摘要替换阅读控制栏与 Dock 上下文操作');
}

async function testReadingToc(page, origin, audit) {
  await page.goto(origin + '/posts/toc-guide');
  const toc = page.locator('reading-toc'), nav = page.getByRole('navigation',{name:'正文目录'});
  check(await nav.getByRole('link').count() === 4, 'SSR headings missing');
  check(await toc.locator('li').nth(1).evaluate(el => el.style.getPropertyValue('--toc-depth')) === '1', 'heading indentation lost');
  const target = await page.locator('.prose h2').nth(1).getAttribute('id');
  await page.locator('.prose h2').nth(1).evaluate(el => scrollTo(0, el.getBoundingClientRect().top + scrollY - 60));
  await page.waitForFunction(id => document.querySelector('reading-toc [aria-current="location"]')?.getAttribute('data-heading-id') === id, target);
  check(await toc.evaluate(el => Math.abs(el.getBoundingClientRect().top - 32) < 2), 'desktop TOC does not follow scroll');
  await audit();
  await page.emulateMedia({colorScheme:'dark'});
  await page.screenshot({path:'/tmp/sloray-reading-desktop.png',fullPage:false});
  await page.setViewportSize({width:3840,height:1080});
  check(await page.locator('main').evaluate(el => el.getBoundingClientRect().width <= 1120), 'ultrawide article width unbounded');
  check(await page.locator('.reading-content').evaluate(el => el.getBoundingClientRect().width <= 760), 'ultrawide prose width unbounded');
  await audit();
  await page.setViewportSize({width:390,height:844});
  await page.waitForFunction(() => !document.querySelector('reading-toc details').open);
  await toc.locator('summary').click();
  await audit();
  await page.screenshot({path:'/tmp/sloray-reading-mobile-menu.png',fullPage:false});
  await nav.getByRole('link',{name:'第三章',exact:true}).click();
  check(await toc.locator('details').getAttribute('open') === null, 'TOC anchor leaves mobile menu expanded');
  await page.waitForFunction(() => document.querySelector('reading-toc [aria-current="location"]')?.textContent === '第三章');
  check((await page.evaluate(() => document.activeElement?.tagName)) === 'H2', 'TOC anchor did not transfer keyboard focus');
  await toc.locator('summary').click(); await toc.locator('summary').press('Escape');
  check(await toc.locator('details').getAttribute('open') === null, 'Escape leaves TOC expanded');
  await page.locator('.prose').evaluate(el => { const h = document.createElement('h2'); h.textContent = '动态新增章节'; el.append(h); });
  await toc.locator('summary').click();
  await nav.getByRole('link',{name:'动态新增章节',exact:true}).waitFor();
  await page.goto(origin + '/toc-page');
  await page.locator('reading-toc[data-ready="true"]').waitFor();
  check(await page.getByRole('navigation',{name:'正文目录'}).locator('a').count() === 0, 'closed mobile TOC unexpectedly visible');
  await page.locator('reading-toc summary').click();
  check(await page.getByRole('navigation',{name:'正文目录'}).getByRole('link').count() === 4, 'page TOC missing');
  await audit();
  const context = await page.context().browser().newContext({javaScriptEnabled:false,viewport:{width:1280,height:900}});
  try {
    const fallback = await context.newPage(); await fallback.goto(origin + '/toc-page');
    check(await fallback.getByRole('navigation',{name:'正文目录'}).getByRole('link').count() === 4, 'no-JavaScript SSR TOC lost');
  } finally { await context.close(); }
  await page.emulateMedia({colorScheme:'light'});
  console.log('PASS 文章/页面 SSR 目录、桌面随动/章节高亮、手机展开/锚点焦点/Escape、动态标题重载、3840px 宽度与无 JS 目录');
}

export async function testIdentity(page, context, origin, audit) {
  await page.goto(origin + '/admin/settings');
  await page.getByLabel('站点名称',{exact:true}).fill('微光书房');
  await page.getByLabel('Favicon 图片地址',{exact:true}).fill('/favicon.svg');
  await page.getByRole('button',{name:'保存站点标识',exact:true}).click();
  await page.locator('.toast[data-kind=success]').getByText('站点标识已保存，公开页面刷新后生效。',{exact:true}).waitFor();
  await audit();
  await page.goto(origin);
  check(await page.title() === '微光书房', 'site title not configured');
  check(await page.locator('link[rel=icon]').getAttribute('href') === '/favicon.svg', 'favicon not configured');
  check((await (await context.request.get(origin + '/rss.xml')).text()).includes('<title>微光书房</title>'), 'RSS title not configured');
  await page.goto(origin + '/admin/settings');
  await page.getByLabel('站点名称',{exact:true}).fill('静线数据库');
  await page.getByLabel('Favicon 图片地址',{exact:true}).fill('');
  await page.getByRole('button',{name:'保存站点标识',exact:true}).click();
  await page.locator('.toast[data-kind=success]').getByText('站点标识已保存，公开页面刷新后生效。',{exact:true}).waitFor();
  console.log('PASS 设置页站点名称/Favicon 保存、页面标题/导航/RSS 同步与恢复默认');
}

async function testDockActions(page, origin, audit) {
  check(await page.locator('site-dock [data-dock-key=home] img').getAttribute('src') === '/favicon.svg', 'home favicon is outside Dock');
  await page.getByRole('button',{name:'评论',exact:true}).click();
  await page.locator('#comment-body').fill('保留的评论草稿');
  await page.screenshot({path:'/tmp/sloray-comment-dialog.png'});
  await audit();
  await page.getByRole('button',{name:'关闭评论',exact:true}).click();
  page.once('dialog', dialog => dialog.dismiss());
  await page.getByRole('link',{name:'主页',exact:true}).click();
  check(page.url().endsWith('/posts/browser-seed'), 'cancelled comment navigation changed page');
  check(await page.locator('[data-comment-dialog]').evaluate(el => el.open), 'cancel did not retain modal');
  check(await page.locator('#comment-body').inputValue() === '保留的评论草稿', 'cancel lost comment draft');
  await page.getByRole('button',{name:'关闭评论',exact:true}).click();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('link',{name:'主页',exact:true}).click();
  await page.waitForURL(origin + '/');
  check(await page.locator('site-dock [data-like],site-dock [data-comment-open]').count() === 0, 'post actions leaked to home');
  await page.evaluate(() => { window.__dockNode = document.querySelector('site-dock'); window.__homeNode = document.querySelector('[data-dock-key=home]'); window.__dockMotion = window.__homeNode.animate([{rotate:'0deg'},{rotate:'360deg'}], {duration:60000, iterations:Infinity}); window.__dockMotion.currentTime = 10000; });
  await page.getByRole('link',{name:'浏览器测试文章',exact:true}).click();
  await page.locator('.discussion[data-bound=true]').waitFor();
  check(await page.evaluate(() => window.__dockNode === document.querySelector('site-dock') && window.__homeNode === document.querySelector('[data-dock-key=home]')), 'Dock nodes replaced during AJAX navigation');
  check(await page.evaluate(() => window.__dockMotion.currentTime >= 10000 && window.__dockMotion.playState === 'running'), 'Dock animation timeline reset');
  await page.evaluate(() => window.__dockMotion.cancel());
  await page.getByRole('button',{name:'评论',exact:true}).click();
  await page.locator('#comment-body').fill('前进后退保留的评论');
  await page.getByRole('button',{name:'关闭评论',exact:true}).click();
  const historyPrompt = page.waitForEvent('dialog');
  await page.evaluate(() => history.back());
  const prompt = await historyPrompt; await prompt.dismiss();
  await page.waitForFunction(() => location.pathname === '/posts/browser-seed' && document.querySelector('[data-comment-dialog]').open);
  check(await page.locator('#comment-body').inputValue() === '前进后退保留的评论', 'browser back lost comment draft');
  await page.getByRole('button',{name:'关闭评论',exact:true}).click();
  const discardPrompt = page.waitForEvent('dialog');
  await page.evaluate(() => history.back());
  const discard = await discardPrompt; await discard.accept();
  await page.waitForURL(origin + '/');
  await page.goForward();
  await page.locator('.discussion[data-bound=true]').waitFor();
  await page.getByRole('link',{name:'搜索',exact:true}).click();
  await page.waitForTimeout(400);
  check(page.url().endsWith('/posts/browser-seed'), 'opening search navigated before confirmation');
  check(await page.locator('[data-search-dialog]').evaluate(el => getComputedStyle(el,'::backdrop').backgroundColor) === 'rgba(0, 0, 0, 0.85)', 'search backdrop is not black');
  await audit();
  await page.screenshot({path:'/tmp/sloray-search-dialog.png'});
  await page.locator('#search-dialog-q').press('Escape');
  await page.locator('[data-search-dialog]').waitFor({state:'hidden'});
  await page.setViewportSize({width:320,height:844});
  await page.waitForFunction(() => [...document.querySelectorAll('site-dock nav a')].filter(link => !link.hidden).length === 2);
  check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.querySelector('.dock-panel').scrollWidth <= document.querySelector('.dock-panel').clientWidth), 'post Dock overflows 320px');
  await page.getByRole('button',{name:'评论',exact:true}).waitFor();
  check(await page.getByRole('button',{name:'评论',exact:true}).isVisible(), 'mobile collapsed Dock hides comment');
  await page.setViewportSize({width:1280,height:720});
  await page.screenshot({path:'/tmp/sloray-contextual-article.png'});
  console.log('PASS Favicon 主页、按页面切换操作、评论取消/放弃草稿、黑色搜索弹窗、320px Dock、AJAX 节点与动画时间线连续');
}

async function testDockFallback(browser, origin) {
  const context = await browser.newContext();
  await context.addInitScript(() => { Element.prototype.moveBefore = undefined; });
  try {
    const page = await context.newPage(); await page.goto(origin);
    await page.locator('site-dock[data-ready=true]').waitFor();
    await page.evaluate(() => {
      window.__dock = document.querySelector('site-dock'); window.__home = document.querySelector('[data-dock-key=home]');
      const ripple = document.createElement('span'); ripple.className = 'touch-ripple'; ripple.style.animationDuration = '60s'; window.__home.append(ripple);
      window.__ripple = ripple; ripple.getAnimations()[0].currentTime = 7000;
    });
    await page.getByRole('link',{name:'浏览器测试文章',exact:true}).click();
    await page.locator('.discussion[data-bound=true]').waitFor();
    check(await page.evaluate(() => window.__dock === document.querySelector('site-dock') && window.__home === document.querySelector('[data-dock-key=home]') && window.__ripple.getAnimations()[0]?.currentTime >= 7000), 'fallback browser reset persisted CSS animation');
    await page.getByRole('link',{name:'搜索',exact:true}).click();
    await page.locator('[data-search-dialog]').waitFor();
    check(page.url().endsWith('/posts/browser-seed'), 'fallback browser duplicated navigation listeners');
    console.log('PASS 禁用 moveBefore 的旧浏览器路径保持 Dock 节点、CSS 动画时间及弹窗监听');
  } finally { await context.close(); }
}
