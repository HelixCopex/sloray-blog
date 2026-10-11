function check(value, message) { if (!value) throw new Error(message); }
export async function testAdminChrome(page, origin, audit) {
  await page.goto(origin + '/admin/posts');
  check(await page.getByRole('link',{name:'管理后台',exact:true}).locator('svg').getAttribute('data-icon') === 'server', 'admin Dock is not a server icon');
  await page.evaluate(() => {
    window.__adminTabs = document.querySelector('.admin-bar');
    const start = document.startViewTransition.bind(document);
    document.startViewTransition = (...args) => {
      const transition = start(...args);
      transition.ready.then(() => { window.__snapshotSkipped = false; }, () => { window.__snapshotSkipped = true; });
      return transition;
    };
  });
  await page.getByRole('navigation',{name:'后台导航'}).getByRole('link',{name:'媒体',exact:true}).click();
  await page.locator('[data-media-picker][data-bound=true]').waitFor({state:'attached'});
  await page.waitForFunction(() => window.__snapshotSkipped !== undefined);
  check(await page.evaluate(() => window.__adminTabs === document.querySelector('.admin-bar') && !document.querySelector('#content').contains(window.__adminTabs)), 'admin tabs were animated or replaced');
  check(await page.evaluate(() => window.__snapshotSkipped), 'native screenshot overlay is still active');
  check(await page.getByRole('navigation',{name:'后台导航'}).getByRole('link',{name:'媒体',exact:true}).getAttribute('aria-current') === 'page', 'persistent admin tab state is stale');
  check(!await page.locator('[data-upload]').isEnabled(), 'empty upload must be disabled');
  await page.locator('#image-file').setInputFiles({name:'invalid.txt',mimeType:'text/plain',buffer:Buffer.from('test')});
  await page.locator('[data-upload]').click();
  await page.locator('.toast[data-kind=error]').filter({hasText:'1 张上传失败'}).waitFor();
  check(await page.locator('toast-stack').evaluate(el => el.matches(':popover-open')), 'toast is not in the top layer');
  await audit();
  await page.getByRole('navigation',{name:'后台导航'}).getByRole('link',{name:'主题',exact:true}).click();
  await page.locator('theme-workbench[data-bound=true]').waitFor();
  check(await page.evaluate(() => window.__adminTabs === document.querySelector('.admin-bar')), 'theme island replaced admin navigation');
  await audit();
  await page.goto(origin + '/toc-page');
  check(await page.locator('[data-reading-progress]').count() === 0, 'custom page still has reading progress');
  await page.goto(origin + '/posts/toc-guide');
  check(await page.locator('[data-reading-progress]').evaluate(el => getComputedStyle(el).backgroundColor === 'rgba(0, 0, 0, 0)'), 'progress has a background track');
  await page.goto(origin + '/admin/posts/new');
  await page.locator('#editor[data-bound=true]').waitFor();
  await page.locator('site-dock[data-ready=true]').waitFor();
  await page.getByRole('button',{name:'打开媒体选择器',exact:true}).click();
  await page.locator('#image-file').setInputFiles({name:'invalid.txt',mimeType:'text/plain',buffer:Buffer.from('test')});
  await page.locator('[data-upload]').click();
  const modalNotice = page.locator('.toast[data-kind=error]').filter({hasText:'1 张上传失败'});
  await modalNotice.waitFor();
  await modalNotice.getByRole('button',{name:'关闭通知',exact:true}).click();
  check(await modalNotice.count() === 0, 'toast above a modal cannot be dismissed');
  await page.locator('#media-picker').getByRole('button',{name:'关闭',exact:true}).click();
  await page.locator('#title').fill('不可达图片仍可保存'); await page.locator('#slug').fill('unreachable-image');
  await page.locator('#body').fill('![离线图片](/unreachable-test-image.png)');
  await page.route('**/unreachable-test-image.png', route => route.abort('failed'));
  await page.getByRole('button',{name:'保存草稿',exact:true}).click();
  await page.waitForURL(/\/admin\/posts\/[0-9a-f-]+$/);
  await page.locator('.toast[data-kind=warning]').filter({hasText:'已保存正文'}).waitFor();
  check(await page.locator('#body').inputValue() === '![离线图片](/unreachable-test-image.png)', 'failed dimension lookup changed body');
  await page.unroute('**/unreachable-test-image.png');
  await page.goto(origin + '/admin/posts');
  console.log('PASS 后台 Tab 持久且位于动效外、跳过快照、服务器图标、页面无进度/文章透明轨道、Toast 颜色/顶层、不可达图片不阻断保存');
}
