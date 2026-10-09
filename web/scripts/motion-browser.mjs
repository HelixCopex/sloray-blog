import fs from 'node:fs';
function check(value, message) { if (!value) throw new Error(message); }
export async function testMotion(page, origin, audit) {
  await page.setViewportSize({width:1280,height:900});
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.goto(origin);
  await page.locator('site-dock[data-ready=true]').waitFor();
  await page.evaluate(() => {
    window.__motionSnapshots = [];
    const start = document.startViewTransition.bind(document);
    document.startViewTransition = (...args) => {
      const transition = start(...args);
      transition.ready.then(() => {
        const animations = document.getAnimations().filter(a => a instanceof CSSAnimation && /page-(back-)?(away|arrive)/.test(a.animationName));
        window.__motionSnapshots.push(animations.map(a => ({name:a.animationName, timing:a.effect.getTiming(), frames:a.effect.getKeyframes()})));
        window.__oldVisibility = getComputedStyle(document.documentElement,'::view-transition-old(page-content)').visibility;
      });
      return transition;
    };
    document.addEventListener('astro:after-swap', () => {
      window.__pendingDockActions = [...document.querySelectorAll('[data-dock-enter=pending]')].map(el => el.dataset.dockKey);
      requestAnimationFrame(() => {
        window.__dockEnter = document.querySelector('[data-like]')?.getAnimations().map(a => ({name:a.animationName,timing:a.effect.getTiming(),frames:a.effect.getKeyframes()}));
      });
    });
  });
  await page.getByRole('link',{name:'浏览器测试文章',exact:true}).click();
  await page.locator('.discussion[data-bound=true]').waitFor();
  await page.waitForFunction(() => window.__motionSnapshots.length > 0);
  const forward = await page.evaluate(() => window.__motionSnapshots[0]);
  const away = forward.find(a => a.name === 'page-away'), arrive = forward.find(a => a.name === 'page-arrive');
  check(away && arrive, 'page content slide/fade animations missing');
  check(away.timing.duration === 200 && away.frames[0].easing === 'cubic-bezier(0.3, 0, 0.8, 0.15)', 'outgoing MD3 curve missing: ' + JSON.stringify(away));
  check(arrive.timing.duration === 350 && arrive.frames[0].easing === 'cubic-bezier(0.05, 0.7, 0.1, 1)', 'incoming MD3 curve missing: ' + JSON.stringify(arrive));
  check(away.frames.at(-1).opacity === '0' && arrive.frames[0].opacity === '0', 'page fade missing');
  check([...away.frames,...arrive.frames].every(f => !f.transform.includes('scale')), 'page still scales');
  check(await page.evaluate(() => window.__oldVisibility === 'visible'), 'departing content is hidden');
  check(await page.evaluate(() => window.__pendingDockActions.includes('like') && !window.__pendingDockActions.includes('home')), 'retained Dock actions were reanimated');
  await page.waitForFunction(() => window.__dockEnter?.some(a => a.name === 'dock-action-enter'));
  check(await page.evaluate(() => window.__dockEnter.find(a => a.name === 'dock-action-enter').frames[0].easing === 'cubic-bezier(0.05, 0.7, 0.1, 1)'), 'new Dock actions do not use MD3 easing');
  await page.waitForFunction(() => !document.querySelector('[data-dock-enter=active]'));
  await page.goBack();
  await page.waitForFunction(() => window.__motionSnapshots.length > 1);
  check(await page.evaluate(() => window.__motionSnapshots[1].some(a => a.name === 'page-back-arrive')), 'history slide direction missing');
  await page.getByRole('link',{name:'浏览器测试文章',exact:true}).waitFor();

  const imageBytes = fs.readFileSync(process.env.SLORAY_BROWSER_IMAGE);
  let release;
  const delayed = new Promise(resolve => { release = resolve; });
  await page.route('**/test-focus.png', async route => { await delayed; await route.fulfill({contentType:'image/png',body:imageBytes}); });
  await page.route('**/test-portrait.png', route => route.abort('failed'));
  await page.goto(origin + '/posts/image-focus', {waitUntil:'domcontentloaded'});
  const frame = page.locator('.prose focus-image').first();
  await frame.waitFor();
  await frame.scrollIntoViewIfNeeded();
  const before = await frame.boundingBox();
  check(Math.abs(before.width / before.height - 2) < .01, 'placeholder does not match stored aspect ratio');
  check(await frame.evaluate(el => getComputedStyle(el.querySelector('img')).opacity === '0' && getComputedStyle(el,'::after').opacity === '1'), 'loaded image is visible before decode');
  const paragraph = page.getByText('占位之后的正文。',{exact:true});
  const paragraphBefore = await paragraph.boundingBox();
  await page.screenshot({path:'/tmp/sloray-image-placeholder.png'});
  release();
  await frame.locator('img').evaluate(img => img.decode());
  await page.waitForFunction(() => document.querySelector('.prose focus-image')?.dataset.state === 'ready');
  const focus = await frame.evaluate(el => {
    const img = el.querySelector('img');
    const transition = img.getAnimations().find(a => a.transitionProperty === 'filter');
    if (!transition) return null;
    el.getAnimations({subtree:true}).forEach(a => { a.pause(); a.currentTime = 100; });
    return {filter:getComputedStyle(img).filter,easing:getComputedStyle(img).transitionTimingFunction.match(/^cubic-bezier\([^)]+\)/)?.[0]};
  });
  check(focus?.filter.startsWith('blur(') && focus.filter !== 'blur(24px)' && focus.filter !== 'blur(0px)', 'image did not progressively focus');
  check(focus.easing === 'cubic-bezier(0.05, 0.7, 0.1, 1)', 'image focus easing is not MD3');
  await page.screenshot({path:'/tmp/sloray-image-focusing.png'});
  await frame.evaluate(el => el.getAnimations({subtree:true}).forEach(a => a.finish()));
  const after = await frame.boundingBox(), paragraphAfter = await paragraph.boundingBox();
  check(Math.abs(after.height-before.height) < .1 && Math.abs(paragraphAfter.y-paragraphBefore.y) < .1, 'image loading shifted document layout');
  check(await frame.evaluate(el => getComputedStyle(el.querySelector('img')).filter === 'none' && getComputedStyle(el.querySelector('img')).opacity === '1' && getComputedStyle(el,'::after').opacity === '0'), 'image remains blurred or covered');
  await page.screenshot({path:'/tmp/sloray-image-loaded.png'});
  const portrait = page.locator('.prose focus-image').nth(1);
  await portrait.scrollIntoViewIfNeeded();
  await page.waitForFunction(() => document.querySelectorAll('.prose focus-image')[1]?.dataset.state === 'error');
  const portraitBox = await portrait.boundingBox();
  check(Math.abs(portraitBox.width / portraitBox.height - .5) < .01, 'failed portrait lost reserved original ratio');
  await audit();
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.goto(origin + '/posts/image-focus');
  await page.waitForFunction(() => document.querySelector('.prose focus-image')?.dataset.state === 'ready');
  check(await frame.evaluate(el => getComputedStyle(el.querySelector('img')).filter === 'none' && getComputedStyle(el.querySelector('img')).transitionDuration === '0s'), 'reduced motion retains focus animation');
  await page.emulateMedia({reducedMotion:'no-preference'});
  await page.unroute('**/test-focus.png'); await page.unroute('**/test-portrait.png');
  const fallbackContext = await page.context().browser().newContext({javaScriptEnabled:false});
  try {
    const fallback = await fallbackContext.newPage();
    await fallback.route('**/test-focus.png', route => route.fulfill({contentType:'image/png',body:imageBytes}));
    await fallback.goto(origin + '/posts/image-focus');
    check(await fallback.locator('.prose focus-image img').first().evaluate(img => getComputedStyle(img).opacity === '1' && img.naturalWidth === 400), 'no-JavaScript content image invisible');
  } finally { await fallbackContext.close(); }
  await page.goto(origin);
  console.log('PASS MD3 前进/返回滑移淡入淡出、无缩放、Dock 新按钮缓动、图片原比例占位/聚焦/零漂移/失败、减少动态效果与无 JS');
}
