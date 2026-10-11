import assert from 'node:assert/strict';

async function waitSettingsDock(page, open) {
  // setViewportSize resolves before the page's matchMedia change handler may run.
  await page.waitForFunction(expected => document.querySelector('.settings-dock')?.open === expected, open);
}

async function responsive(page, audit, name) {
  for (const colorScheme of ['light', 'dark']) {
    await page.emulateMedia({colorScheme});
    for (const width of [1280, 390, 320]) {
      await page.setViewportSize({width, height:width === 1280 ? 900 : 844});
      if (name === 'settings') {
        await waitSettingsDock(page, width === 1280);
        assert.equal(await page.locator('.settings-dock').evaluate(node => node.open), width === 1280, 'settings navigation must collapse on mobile');
        if (width !== 1280) {
          const toggle = page.locator('.settings-dock summary');
          await toggle.focus(); await page.keyboard.press('Enter');
          assert.ok(await page.locator('.settings-nav a[href="/admin/media"]').isVisible());
          await page.keyboard.press('Enter');
          assert.equal(await page.locator('.settings-dock').evaluate(node => node.open), false);
        }
      }
      await audit();
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${name} overflows ${width}px`);
      if (width !== 320) await page.screenshot({path:`/tmp/sloray-${name}-${colorScheme}-${width}.png`,fullPage:true});
    }
  }
  await page.setViewportSize({width:1280,height:720});
  if (name === 'settings') await waitSettingsDock(page, true);
  await page.emulateMedia({colorScheme:'light'});
}

export async function testToastDesign(page, audit) {
  await page.locator('#email').focus();
  await page.evaluate(() => {
    for (const kind of ['info', 'success', 'warning', 'error']) window.dispatchEvent(new CustomEvent('sloray:toast', {
      detail:{kind,id:`design-${kind}`,message:`${({info:'信息通知',success:'已保存',warning:'请检查输入',error:'操作未完成'})[kind]}：测试 <strong>文字</strong> 按原样显示。`},
    }));
    // Pause all four existing timers while responsive audits inspect their styles.
    document.querySelectorAll('.toast').forEach(node => node.dispatchEvent(new PointerEvent('pointerenter')));
  });
  assert.equal(await page.locator('.toast').count(), 4);
  assert.equal(await page.locator('.toast strong').count(), 0, 'toast content must remain plain text');
  assert.equal(await page.locator('.toast[data-kind=error]').getAttribute('role'), 'alert');
  assert.equal(await page.locator('.toast[data-kind=success]').getAttribute('role'), 'status');
  for (const colorScheme of ['light', 'dark']) {
    await page.emulateMedia({colorScheme});
    await page.setViewportSize({width:1280,height:844});
    await audit();
    await page.screenshot({path:`/tmp/sloray-toasts-${colorScheme}.png`});
    const colors = await page.locator('.toast').evaluateAll(nodes => nodes.map(node => getComputedStyle(node).backgroundColor));
    assert.equal(new Set(colors).size, 4, 'notification kinds must remain visually distinguishable');
    for (const width of [1280, 320]) {
      await page.setViewportSize({width,height:844});
      const bounds = await page.locator('.toast-close').evaluateAll(nodes => nodes.map(node => {
        const rect = node.getBoundingClientRect(); return {width:rect.width,height:rect.height,left:rect.left,right:rect.right};
      }));
      assert.ok(bounds.every(box => box.width >= 48 && box.height >= 48 && box.left >= 0 && box.right <= width));
      await audit();
    }
  }
  await page.locator('.toast[data-kind=error] .toast-close').focus();
  await page.keyboard.press('Enter');
  assert.equal(await page.locator('.toast[data-kind=error]').count(), 0);
  assert.ok(await page.locator('#email').evaluate(node => node === document.activeElement), 'closing a focused notification must restore focus');
  while (await page.locator('.toast-close').count()) await page.locator('.toast-close').first().click();
  await page.setViewportSize({width:1280,height:720});
  await page.emulateMedia({colorScheme:'light'});
}

export async function testLoginDesign(page, origin, audit) {
  await page.goto(origin + '/login');
  await page.evaluate(() => localStorage.removeItem('sloray-appearance'));
  await page.reload();
  await responsive(page, audit, 'login');
  for (const appearance of ['dark', 'light']) {
    await page.evaluate(value => localStorage.setItem('sloray-appearance', value), appearance);
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-appearance'), appearance, 'Rust login lost the saved appearance');
  }
  await page.evaluate(() => localStorage.removeItem('sloray-appearance'));
  await page.reload();
  await page.locator('#email').fill('admin@example.com');
  await page.locator('#password').fill('incorrect-test-password');
  await page.getByRole('button',{name:'登录',exact:true}).click();
  await page.locator('.toast[data-kind=error]').filter({hasText:'邮箱或密码不正确。'}).waitFor();
  assert.equal(await page.locator('#email').inputValue(), 'admin@example.com');
  assert.equal(await page.locator('#password').inputValue(), '');
  await testToastDesign(page, audit);
  const fallbackContext = await page.context().browser().newContext({javaScriptEnabled:false,viewport:{width:390,height:844}});
  try {
    const fallback = await fallbackContext.newPage();
    await fallback.goto(origin + '/login');
    await fallback.locator('#email').fill('admin@example.com');
    await fallback.locator('#password').fill('incorrect-test-password');
    await fallback.getByRole('button',{name:'登录',exact:true}).click();
    await fallback.locator('.notice.err').filter({hasText:'邮箱或密码不正确。'}).waitFor();
    assert.equal(await fallback.locator('#password').inputValue(), '');
  } finally { await fallbackContext.close(); }
  console.log('PASS 登录页浅深色/320/390/1280px axe、错误保留邮箱不回显密码、无 JS 反馈、Toast 四类表面/48px关闭/纯文本与键盘焦点恢复');
}

export async function testSettingsDesign(page, origin, audit) {
  await page.goto(origin + '/admin/settings');
  await responsive(page, audit, 'settings');
  await page.setViewportSize({width:390,height:844});
  await waitSettingsDock(page, false);
  await page.locator('.settings-dock summary').click();
  await page.setViewportSize({width:320,height:844});
  assert.ok(await page.locator('.settings-dock').evaluate(node => node.open), 'resizing within the mobile breakpoint lost the chosen navigation state');
  await page.setViewportSize({width:1280,height:720});
  await waitSettingsDock(page, true);
  await page.locator('.settings-nav a[href="/admin/media"]').waitFor({state:'visible'});
  assert.ok(await page.locator('.settings-nav a[href="/admin/media"]').isVisible(), 'returning to desktop hid navigation');
  await page.evaluate(() => localStorage.setItem('sloray-appearance', 'dark'));
  await page.reload();
  assert.equal(await page.locator('html').getAttribute('data-appearance'), 'dark', 'Rust settings lost the saved appearance');
  await page.evaluate(() => localStorage.removeItem('sloray-appearance'));
  await page.reload();
  for (const selector of ['#db_url', '#s3_secret_access_key', '#cdn_token']) assert.equal(await page.locator(selector).inputValue(), '', 'saved credentials must never be echoed');
  const form = page.locator('form[action="/admin/settings"]');
  const previous = await form.evaluate(node => Object.fromEntries(new FormData(node)));
  await form.getByRole('button',{name:'保存',exact:true}).click();
  await page.locator('.toast[data-kind=success]').getByText('已保存。',{exact:true}).waitFor();
  assert.deepEqual(await page.locator('form[action="/admin/settings"]').evaluate(node => Object.fromEntries(new FormData(node))), previous, 'saving unchanged runtime settings changed the displayed configuration');
  await audit();
  const fallbackContext = await page.context().browser().newContext({javaScriptEnabled:false,viewport:{width:390,height:844},storageState:await page.context().storageState()});
  try {
    const fallback = await fallbackContext.newPage(); await fallback.goto(origin + '/admin/settings');
    assert.ok(await fallback.locator('.settings-nav a[href="/admin/media"]').isVisible(), 'no-JavaScript navigation is unavailable');
  } finally { await fallbackContext.close(); }
  console.log('PASS 设置页浅深色/320/390/1280px axe、手机导航折叠/键盘展开/断点同步/无 JS 可用、运行时配置原生保存、密钥留空与保存反馈');
}
