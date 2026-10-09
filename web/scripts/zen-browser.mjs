/** Native Zen regression through its built-in WebDriver BiDi; uses an isolated temporary profile. */
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn } from 'node:child_process';
import { landmarkBounds } from './zen-pixels.mjs';
const origin = process.env.SLORAY_BROWSER_URL;
const profile = await fs.mkdtemp(path.join(os.tmpdir(),'sloray-zen-'));
const server = net.createServer(); await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
const port = server.address().port; await new Promise(resolve => server.close(resolve));
const processZen = spawn(process.env.ZEN_BIN ?? 'zen-browser', ['--headless','--new-instance','--profile',profile,'--remote-debugging-port',String(port),'about:blank'], {stdio:'ignore'});
const delay = ms => new Promise(resolve => setTimeout(resolve,ms));
let socket, context, serial = 0;
const pending = new Map();
async function connect() {
  for (let attempt=0;attempt<40;attempt++) {
    try {
      socket = await new Promise((resolve,reject) => {
        const connection = new WebSocket(`ws://127.0.0.1:${port}/session`);
        connection.addEventListener('open',() => resolve(connection),{once:true});
        connection.addEventListener('error',reject,{once:true});
      });
      return;
    } catch { await delay(250); }
  }
  throw new Error('独立 Zen WebDriver BiDi 启动失败');
}
function command(method,params) {
  return new Promise((resolve,reject) => {
    const id = ++serial;
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`${method} 超时`)); },20000);
    pending.set(id,{resolve,reject,timeout}); socket.send(JSON.stringify({id,method,params}));
  });
}
async function evaluate(expression) {
  const result = await command('script.evaluate',{expression,target:{context},awaitPromise:true});
  if (result.type === 'exception') throw new Error(result.exceptionDetails.text);
  return result.result.value;
}
async function wait(expression) {
  for (let i=0;i<100;i++) { try { if (await evaluate(expression)) return; } catch {} await delay(100); }
  console.log('Zen timeout state: ' + await evaluate(`JSON.stringify({path:location.pathname,reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,motion:window.__zenMotion?.map(m=>({id:m.id,state:m.animation.playState,time:m.animation.currentTime})),events:window.__zenEvents,errors:window.__zenErrors})`));
  throw new Error('Zen 条件未满足：' + expression);
}
function check(value,message) { if (!value) throw new Error(message); }
// These browser-side probes stop actual DOM animation, including hydration that continues while paused.
function installMotionProbe(target, long) {
  window.__zenTabs = document.querySelector('.admin-bar');
  window.__zenDock = document.querySelector('site-dock');
  window.__zenMotion = []; window.__zenNative = [];
  window.__zenErrors = []; window.__zenEvents = [];
  window.addEventListener('error', event => window.__zenErrors.push(event.message));
  window.addEventListener('unhandledrejection', event => window.__zenErrors.push(String(event.reason)));
  document.addEventListener('astro:before-preparation', event => window.__zenEvents.push(event.type + ':' + event.direction));
  const animate = Element.prototype.animate;
  Element.prototype.animate = function(frames, options) {
    const animation = animate.call(this, frames, options);
    if (this.id === 'content' && /page-(back-)?(away|arrive)/.test(options?.id ?? '')) {
      animation.pause(); animation.currentTime = 120;
      window.__zenMotion.push({animation, main:this, id:options.id});
    }
    return animation;
  };
  const start = document.startViewTransition.bind(document);
  document.startViewTransition = (...args) => {
    const transition = start(...args);
    transition.ready.then(() => window.__zenNative.push('captured'), () => window.__zenNative.push('skipped'));
    return transition;
  };
  const main = document.querySelector('#content'), marker = document.createElement('div');
  marker.dataset.zenMarker = 'old';
  marker.style.cssText = 'width:240px;height:64px;background:rgb(224,46,143);margin:0 0 16px';
  main.prepend(marker);
  if (long) { const tail=document.createElement('div'); tail.style.height='6000px'; main.append(tail); }
  window.__zenOldHeight = main.getBoundingClientRect().height;
  document.addEventListener('astro:before-swap', event => {
    const next=document.createElement('div'); next.dataset.zenMarker='new';
    next.style.cssText='width:240px;height:64px;background:rgb(21,205,180);margin:0 0 16px';
    event.newDocument.querySelector('#content').prepend(next);
  }, {once:true});
  let link = [...document.querySelectorAll('a')].find(a => a.getAttribute('href') === target);
  if (!link) { link=document.createElement('a'); link.href=target; document.body.append(link); }
  link.click();
  return true;
}
try {
  await connect();
  socket.addEventListener('message',event => {
    const message = JSON.parse(event.data), request = pending.get(message.id);
    if (!request) return;
    clearTimeout(request.timeout); pending.delete(message.id);
    if (message.type === 'error') request.reject(new Error(message.message)); else request.resolve(message.result);
  });
  const session = await command('session.new',{capabilities:{}});
  ({context} = await command('browsingContext.create',{type:'tab'}));
  await command('browsingContext.setViewport',{context,viewport:{width:1280,height:900},devicePixelRatio:1});
  await command('browsingContext.navigate',{context,url:origin+'/login',wait:'complete'});
  await evaluate(`document.querySelector('[name=email]').value='admin@example.com'; document.querySelector('[name=password]').value='a-test-only-password'; document.querySelector('form').requestSubmit(); true`);
  await wait(`location.pathname==='/admin/posts' && !!document.querySelector('site-dock[data-ready=true]')`);
  await wait(`!!document.querySelector('appearance-control[data-bound=true]')`);
  await evaluate(`const appearance=document.querySelector('[data-appearance-toggle]'); appearance.click(); appearance.click(); true`);
  check(await evaluate(`document.documentElement.dataset.appearance==='dark'`),'Zen 深色外观未设置');
  async function transitionCase(from,to,name,{long=false}={}) {
    // Full navigation resets the page, then instrument only the real client navigation under test.
    await command('browsingContext.navigate',{context,url:origin+from,wait:'complete'});
    await wait(`!!document.querySelector('site-dock[data-ready=true]')`);
    // Astro's page modules are async: Dock readiness alone precedes the BaseLayout interaction bundle.
    await wait(`!!customElements.get('focus-image')`);
    if(from==='/admin/themes') await wait(`!!document.querySelector('theme-workbench[data-bound=true]')`);
    await evaluate(`(${installMotionProbe.toString()})(${JSON.stringify(to)}, ${long})`);
    await wait(`window.__zenMotion.some(m=>m.id==='page-away')`);
    await evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))).then(()=>true)`);
    const departing=JSON.parse(await evaluate(`JSON.stringify({path:location.pathname,motion:window.__zenMotion.map(m=>({id:m.id,state:m.animation.playState,time:m.animation.currentTime})),mainCount:document.querySelectorAll('main#content').length,height:document.querySelector('#content').getBoundingClientRect().height,oldHeight:window.__zenOldHeight})`));
    check(departing.path===from && departing.motion.length===1 && departing.motion[0].state==='paused' && departing.motion[0].time===120 && departing.mainCount===1,'Zen 退出中已换页或未暂停：'+JSON.stringify(departing));
    let shot=await command('browsingContext.captureScreenshot',{context,origin:'viewport'}), pixels=Buffer.from(shot.data,'base64');
    await fs.writeFile(`/tmp/sloray-zen-${name}-departing.png`,pixels);
    const oldPixels=landmarkBounds(pixels,'old'), premature=landmarkBounds(pixels,'new');
    check(Math.abs(oldPixels.width-240)<=2 && Math.abs(oldPixels.height-64)<=2 && oldPixels.rgb?.[0]<221 && premature.count===0,'Zen 退出画面像素比例错误或新内容提前绘制：'+JSON.stringify({oldPixels,premature}));
    await evaluate(`window.__zenMotion[0].animation.finish(); true`);
    await wait(`location.pathname===${JSON.stringify(to)} && window.__zenMotion.some(m=>m.id==='page-arrive')`);
    if(to==='/admin/themes') await wait(`!!document.querySelector('theme-workbench[data-bound=true]')`);
    await evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))).then(()=>true)`);
    check(await evaluate(`window.__zenMotion.length===2 && !window.__zenMotion[0].main.isConnected && window.__zenMotion[1].main===document.querySelector('#content') && window.__zenMotion[1].animation.playState==='paused' && window.__zenMotion[1].animation.currentTime===120 && document.querySelectorAll('main#content').length===1`),'Zen 入场截图未停在真实新正文动画中或旧正文仍在 DOM');
    check(await evaluate(`window.__zenDock===document.querySelector('site-dock') && (!window.__zenTabs || (window.__zenTabs===document.querySelector('.admin-bar') && !document.querySelector('#content').contains(window.__zenTabs)))`),'Zen 持久 Dock/Tab 被替换或进入动画');
    check(await evaluate(`window.__zenNative.length>0 && window.__zenNative.every(s=>s==='skipped')`),'Zen 原生快照未被跳过');
    if(to==='/admin/themes') check(await evaluate(`document.querySelector('theme-workbench').getBoundingClientRect().height>1000 && document.querySelector('.theme-preview').getBoundingClientRect().height>600`),'Zen 主题岛屿在入场中被压缩');
    shot=await command('browsingContext.captureScreenshot',{context,origin:'viewport'}); pixels=Buffer.from(shot.data,'base64');
    await fs.writeFile(`/tmp/sloray-zen-${name}-arriving.png`,pixels);
    const newPixels=landmarkBounds(pixels,'new'), lingering=landmarkBounds(pixels,'old');
    check(Math.abs(newPixels.width-240)<=2 && Math.abs(newPixels.height-64)<=2 && newPixels.rgb?.[1]<202 && lingering.count===0,'Zen 入场画面像素比例错误或仍绘制旧内容：'+JSON.stringify({newPixels,lingering}));
    await evaluate(`window.__zenMotion[1].animation.finish(); true`);
    await wait(`document.querySelector('#content').dataset.pageMotion===undefined`);
    console.log(`PASS Zen serial ${name}: departing ${oldPixels.width}×${oldPixels.height}, arriving ${newPixels.width}×${newPixels.height}, no stale pixels`);
  }
  await transitionCase('/','/posts/browser-seed','home-post');
  await transitionCase('/posts/toc-guide','/','long-post-home',{long:true});
  await transitionCase('/admin/themes','/admin/users','themes-users',{long:true});
  await transitionCase('/admin/users','/admin/themes','users-themes');
  await transitionCase('/admin/themes','/admin/media','themes-media',{long:true});
  await command('browsingContext.navigate',{context,url:origin+'/admin/media',wait:'complete'});
  await wait(`!!document.querySelector('[data-media-picker][data-bound=true]')`);
  await evaluate(`document.querySelector('[data-upload]').click(); true`);
  await wait(`!!document.querySelector('.toast[data-kind=warning]')`);
  check(await evaluate(`document.querySelector('toast-stack').matches(':popover-open')`),'Zen Toast 未处于顶层');
  await command('browsingContext.navigate',{context,url:origin+'/admin/posts/new',wait:'complete'});
  await wait(`!!document.querySelector('#editor[data-bound=true]')`);
  await evaluate(`document.querySelector('[data-open-media]').click(); document.querySelector('[data-upload]').click(); true`);
  await wait(`!!document.querySelector('.toast[data-kind=warning]')`);
  check(await evaluate(`(() => { const button=document.querySelector('.toast[data-kind=warning] .toast-close'); const rect=button.getBoundingClientRect(); return document.elementFromPoint(rect.x+rect.width/2,rect.y+rect.height/2)===button; })()`),'Zen 弹窗内 Toast 被遮挡或变为 inert');
  console.log(`PASS Zen ${session.capabilities.browserVersion} 公众/后台切页真实中间帧无重叠/压缩、长短正文原尺寸、后台 Tab/Dock 保持节点、Toast 顶层显示/弹窗内可点击`);
  await command('session.end',{});
} finally {
  socket?.close(); processZen.kill('SIGTERM');
  await Promise.race([new Promise(resolve => processZen.once('exit',resolve)),delay(3000)]);
  if (processZen.exitCode===null) { processZen.kill('SIGKILL'); await delay(250); }
  await fs.rm(profile,{recursive:true,force:true});
}
