function check(value, message) { if (!value) throw new Error(message); }

async function navigate(page, path) {
  await page.evaluate(href => {
    const link = document.createElement('a'); link.href = href;
    document.body.append(link); link.click(); link.remove();
  }, path);
}

async function count(page) { return page.evaluate(() => window.__pageMotionSamples.length); }

async function awaitAnimation(page, previous, phase) {
  await page.waitForFunction(({previous,phase}) => {
    const current = window.__pageMotionSamples.at(-1);
    return window.__pageMotionSamples.length > previous && current?.animation.id.endsWith(phase) && current.animation.playState === 'paused';
  }, {previous,phase});
}

async function finish(page) {
  await page.evaluate(() => window.__pageMotionSamples.at(-1).animation.finish());
}

async function visibleAndClean(page) {
  await page.waitForFunction(() => {
    const main = document.getElementById('content');
    return main && getComputedStyle(main).opacity === '1' && getComputedStyle(main).transform === 'none' && !main.dataset.pageMotion && !main.style.opacity && !main.getAnimations().some(a => a.id.startsWith('page-'));
  });
  check(await page.locator('main').count() === 1 && await page.locator('main h1').isVisible(), 'navigation left duplicate or invisible content');
}

async function dockRetained(page) {
  check(await page.evaluate(() => window.__navigationDock === document.querySelector('site-dock') && window.__navigationHome === document.querySelector('[data-dock-key=home]') && window.__navigationDockMotion.currentTime >= 9000 && window.__navigationDockMotion.playState === 'running'), 'swap fallback replaced Dock nodes or restarted their animation');
}

async function cancelWithGatedNavigation(page, destination) {
  let release, requested;
  const gate = new Promise(resolve => { release = resolve; });
  const request = new Promise(resolve => { requested = resolve; });
  const url = new URL(destination, page.url()).href;
  const handler = async route => { requested(); await gate; await route.continue(); };
  await page.route(url, handler);
  await page.evaluate(() => {
    window.__cancelledPageAnimation = window.__pageMotionSamples.at(-1).animation;
    window.__cancelledPageMain = document.getElementById('content');
  });
  const previous = await count(page);
  try {
    await navigate(page, destination);
    await request;
    await visibleAndClean(page);
    check(await page.evaluate(() => window.__cancelledPageAnimation.playState === 'idle' && window.__cancelledPageMain === document.getElementById('content')), 'superseding a navigation did not cancel and restore the currently visible page');
  } finally { release(); }
  await awaitAnimation(page, previous, 'away');
  await finish(page);
  await awaitAnimation(page, previous + 1, 'arrive');
  await page.waitForURL(url);
  await page.unroute(url, handler);
  check(await page.evaluate(() => !window.__cancelledPageMain.isConnected), 'cancelled navigation later overwrote the final destination');
  await dockRetained(page);
}

/** Exercise the real router in a browser lacking native View Transitions. */
export async function testNavigationMotion(page, origin) {
  const context = await page.context().browser().newContext({viewport:{width:1280,height:900},reducedMotion:'no-preference'});
  await context.addInitScript(() => {
    document.startViewTransition = undefined;
    window.__pageMotionSamples = [];
    const animate = Element.prototype.animate;
    Element.prototype.animate = function(frames, options) {
      const animation = animate.call(this, frames, options);
      if (options?.id?.startsWith('page-')) {
        animation.pause(); animation.currentTime = 100;
        window.__pageMotionSamples.push({animation,element:this});
      }
      return animation;
    };
  });
  const errors = [];
  try {
    const fallback = await context.newPage();
    fallback.on('pageerror', error => errors.push(error.message));
    await fallback.goto(origin);
    await fallback.locator('site-dock[data-ready=true]').waitFor();
    await fallback.evaluate(() => {
      window.__navigationDock = document.querySelector('site-dock');
      window.__navigationHome = document.querySelector('[data-dock-key=home]');
      window.__navigationDockMotion = window.__navigationHome.animate([{rotate:'0deg'},{rotate:'360deg'}],{duration:60000,iterations:Infinity});
      window.__navigationDockMotion.currentTime = 9000;
      window.__navigationProof = 'same document';
    });

    await navigate(fallback, '/posts/browser-seed');
    await awaitAnimation(fallback, 0, 'away');
    check(await fallback.evaluate(() => location.pathname === '/' && !document.querySelector('.discussion') && document.querySelectorAll('main').length === 1), 'fallback showed incoming content before departure finished');
    await finish(fallback);
    await awaitAnimation(fallback, 1, 'arrive');
    check(await fallback.evaluate(() => location.pathname === '/posts/browser-seed' && document.querySelectorAll('main').length === 1 && window.__navigationProof === 'same document'), 'fallback did not perform a single AJAX content swap');
    await finish(fallback); await visibleAndClean(fallback); await dockRetained(fallback);

    // Cancel an outgoing page while the second response is still pending.
    const previous = await count(fallback);
    await navigate(fallback, '/posts/toc-guide');
    await awaitAnimation(fallback, previous, 'away');
    await cancelWithGatedNavigation(fallback, '/toc-page');
    check(await fallback.locator('main h1').textContent() === '页面阅读指南', 'superseded article replaced the custom page');

    // The new page remains readable when its paused entry is cancelled, too.
    await cancelWithGatedNavigation(fallback, '/posts/browser-seed');
    check(await fallback.locator('main h1').textContent() === '浏览器测试文章', 'superseded arrival replaced the latest article');
    await finish(fallback); await visibleAndClean(fallback);

    const reducedCount = await count(fallback);
    await fallback.emulateMedia({reducedMotion:'reduce'});
    await navigate(fallback, '/toc-page');
    await fallback.waitForURL(origin + '/toc-page');
    await visibleAndClean(fallback);
    check(await count(fallback) === reducedCount, 'reduced motion still created page slide/fade animations');
    await dockRetained(fallback);

    // Changing the preference during motion must release the pending swap.
    await fallback.emulateMedia({reducedMotion:'no-preference'});
    await navigate(fallback, '/posts/browser-seed');
    await awaitAnimation(fallback, reducedCount, 'away');
    await fallback.evaluate(() => { window.__preferenceAnimation = window.__pageMotionSamples.at(-1).animation; });
    await fallback.emulateMedia({reducedMotion:'reduce'});
    await fallback.waitForURL(origin + '/posts/browser-seed');
    await visibleAndClean(fallback);
    check(await fallback.evaluate(() => window.__preferenceAnimation.playState === 'idle') && await count(fallback) === reducedCount + 1, 'changing to reduced motion left the content hidden or animated');

    // Observe the real failed loader before Astro starts its full navigation.
    // Evaluating JS inside a held document request would deadlock Chromium:
    // that evaluation waits for the very navigation whose response is held.
    await fallback.emulateMedia({reducedMotion:'no-preference'});
    const failureCount = await count(fallback);
    await navigate(fallback, '/toc-page');
    await awaitAnimation(fallback, failureCount, 'away');
    let beforeReload;
    const target = origin + '/posts/toc-guide?navigation-failure=1';
    await fallback.exposeFunction('__recordFailedNavigation', snapshot => { beforeReload = snapshot; });
    await fallback.evaluate(target => {
      document.addEventListener('astro:before-preparation', event => {
        if (event.to.href !== target) return;
        const loader = event.loader;
        event.loader = async () => {
          await loader();
          const main = document.getElementById('content');
          await window.__recordFailedNavigation({path:location.pathname,prevented:event.defaultPrevented,opacity:getComputedStyle(main).opacity,transform:getComputedStyle(main).transform,style:main.style.opacity,motion:main.dataset.pageMotion,animations:main.getAnimations().filter(a => a.id.startsWith('page-')).length,dock:window.__navigationDock === document.querySelector('site-dock')});
        };
      });
    }, target);
    await fallback.route(target, async route => {
      if (!route.request().isNavigationRequest()) { await route.abort('failed'); return; }
      await route.continue();
    });
    await navigate(fallback, target);
    await fallback.waitForURL(target, {waitUntil:'domcontentloaded'});
    await fallback.locator('site-dock[data-ready=true]').waitFor();
    await visibleAndClean(fallback);
    check(beforeReload?.path === '/posts/browser-seed' && beforeReload.prevented && beforeReload.opacity === '1' && beforeReload.transform === 'none' && !beforeReload.style && !beforeReload.motion && beforeReload.animations === 0 && beforeReload.dock, 'failed fetch left the previous content hidden before full navigation: ' + JSON.stringify(beforeReload));
    check(await fallback.evaluate(() => window.__navigationProof === undefined) && await fallback.locator('main h1').textContent() === '阅读随动指南', 'network failure did not complete the full-navigation fallback');
    check(errors.length === 0, 'navigation fallback browser errors: ' + errors.join('; '));
    console.log('PASS 无原生 View Transitions 的 AJAX 单一正文、连续导航取消退出/进入、Dock 节点/时间线、减少动态效果与失败完整导航前恢复正文');
  } finally { await context.unrouteAll({behavior:'wait'}); await context.close(); }
}
