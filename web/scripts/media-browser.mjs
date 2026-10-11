import fs from 'node:fs';
import assert from 'node:assert/strict';

// Different original hashes with the same deterministic fixture pixels.
function variant(png, label) {
  const type = Buffer.from('tEXt'), data = Buffer.from(`batch\0${label}`);
  const body = Buffer.concat([type, data]);
  let crc = 0xffffffff;
  for (const byte of body) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = crc >>> 1 ^ (crc & 1 ? 0xedb88320 : 0);
  }
  const size = Buffer.alloc(4), checksum = Buffer.alloc(4);
  size.writeUInt32BE(data.length); checksum.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
  return Buffer.concat([png.subarray(0, -12), size, body, checksum, png.subarray(-12)]);
}

export async function testMediaBatch(page, origin, audit) {
  await page.goto(origin + '/admin/media');
  await page.locator('[data-media-picker][data-bound=true]').waitFor();
  assert.equal(await page.locator('#image-file').getAttribute('multiple'), '');
  assert.equal(await page.locator('[data-upload]').isEnabled(), false);
  const png = fs.readFileSync(process.env.SLORAY_BROWSER_IMAGE);
  const initial = await page.locator('[data-media-list] li[data-id]').count();
  let requests = 0;
  await page.route('**/api/v1/admin/media/presign', route => {
    requests++;
    return requests === 2 ? route.fulfill({status:503, contentType:'application/json',
      body:JSON.stringify({error:{code:'unavailable',message:'第二张测试图片暂时失败'}})}) : route.continue();
  });
  await page.locator('#image-file').setInputFiles(['one', 'two', 'three'].map(label => ({
    name:`${label}.png`, mimeType:'image/png', buffer:variant(png, label),
  })));
  await page.getByRole('button', {name:'上传 3 张图片', exact:true}).click();
  await page.locator('[data-upload-status]').filter({hasText:'2 张已就绪，1 张上传失败'}).waitFor({timeout:70000});
  assert.equal(requests, 3, 'failure must not prevent later uploads');
  assert.equal(await page.locator('[data-upload-list] [data-state=ready]').count(), 2);
  assert.equal(await page.locator('[data-media-list] li[data-id]').count(), initial + 2);
  assert.match(await page.locator('[data-media-error-details]').textContent(), /two.png.*\n.*第二张测试图片/s);
  await page.locator('button.media-image focus-image[data-state=ready]').first().waitFor();
  await page.evaluate(() => {
    const button = document.querySelector('button.media-image');
    window.__mediaFocusProof = {button, frame:button.querySelector('focus-image')};
  });
  await page.getByRole('button', {name:'重试失败的 1 张图片', exact:true}).click();
  await page.evaluate(() => window.__mediaFocusProof.button.focus());
  await page.locator('[data-upload-status]').filter({hasText:'1 张已就绪。'}).waitFor({timeout:70000});
  assert.equal(requests, 4, 'retry must only transfer failed images');
  assert.equal(await page.locator('[data-media-list] li[data-id]').count(), initial + 3);
  assert.ok(await page.evaluate(() => {
    const {button,frame} = window.__mediaFocusProof;
    return button.isConnected && document.activeElement === button && button.querySelector('focus-image') === frame;
  }), 'batch update must preserve keyboard focus and decoded image frame');
  await page.unroute('**/api/v1/admin/media/presign');
  assert.equal(await page.locator('#image-file').inputValue(), '');
  assert.equal(await page.locator('[data-upload]').isEnabled(), false);

  const tile = page.locator('.media-image').first();
  assert.equal((await tile.textContent()).trim(), '', 'copy action must show only the image');
  const geometry = await tile.evaluate(element => {
    const box = element.getBoundingClientRect(), style = getComputedStyle(element);
    const frame = element.querySelector('focus-image').getBoundingClientRect();
    return {width:box.width,height:box.height,frameWidth:frame.width,frameHeight:frame.height,border:style.borderWidth,background:style.backgroundColor};
  });
  assert.ok(Math.abs(geometry.width - geometry.height) < 1, 'media tile must be square');
  assert.equal(geometry.border, '0px');
  assert.equal(geometry.background, 'rgb(227, 237, 255)');
  assert.ok(Math.abs(geometry.frameWidth - geometry.frameHeight) < 1);
  await tile.focus(); await page.keyboard.press('Enter');
  assert.match(await page.locator('[data-markdown-copy]').inputValue(), /width=400 height=200/);
  await audit();
  await page.screenshot({path:'/tmp/sloray-media-desktop.png',fullPage:true});
  await page.emulateMedia({colorScheme:'dark'}); await audit();
  await page.screenshot({path:'/tmp/sloray-media-dark.png',fullPage:true});
  await page.emulateMedia({colorScheme:'light'});
  await page.setViewportSize({width:390,height:844}); await audit();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'media page overflows mobile');
  await page.screenshot({path:'/tmp/sloray-media-mobile.png',fullPage:true});
  await page.setViewportSize({width:1280,height:720});

  // Hold a poll until its media has been deleted: its late response must stay invisible.
  let sawPoll, releasePoll;
  const pollStarted = new Promise(resolve => { sawPoll = resolve; });
  const pollReleased = new Promise(resolve => { releasePoll = resolve; });
  const existingUrl = await tile.locator('img').getAttribute('src');
  const pollPattern = /\/api\/v1\/admin\/media\/[0-9a-f-]+$/;
  await page.route(pollPattern, async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch(), item = await response.json();
    sawPoll(item.id); await pollReleased;
    await route.fulfill({response,json:{...item,ready:true,variants:[{width:400,url:existingUrl}]}});
  });
  await page.locator('#image-file').setInputFiles({name:'delete-race.png',mimeType:'image/png',buffer:variant(png,'delete-race')});
  await page.locator('[data-upload]').click();
  const deletedId = await Promise.race([pollStarted, new Promise((_, reject) => {
    setTimeout(() => reject(new Error('media processing poll did not start')), 15000).unref();
  })]);
  page.once('dialog', dialog => dialog.accept());
  await page.locator(`li[data-id="${deletedId}"] [data-action=delete]`).click();
  await page.locator(`li[data-id="${deletedId}"]`).waitFor({state:'detached'});
  releasePoll();
  await page.locator('[data-upload-status]').filter({hasText:'1 张已就绪。'}).waitFor({timeout:10000});
  assert.equal(await page.locator(`li[data-id="${deletedId}"]`).count(), 0, 'late poll resurrected deleted media');
  await page.unroute(pollPattern);

  await page.goto(origin + '/admin/posts/new');
  await page.locator('site-dock[data-ready=true]').waitFor();
  await page.getByRole('button', {name:'打开媒体选择器',exact:true}).click();
  await page.locator('.media-image').first().waitFor();
  await audit();
  await page.screenshot({path:'/tmp/sloray-media-picker.png',fullPage:true});
  await page.setViewportSize({width:390,height:844}); await audit();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'picker overflows mobile');
  await page.screenshot({path:'/tmp/sloray-media-picker-mobile.png',fullPage:true});
  await page.setViewportSize({width:1280,height:720});
  await page.getByRole('button', {name:'关闭',exact:true}).click();
  console.log('PASS 多图上传、单张失败不中断、仅重试失败项、焦点/解码节点保留、删除轮询竞态、图片方形无边框与浅深色/手机 axe 检查');
}
