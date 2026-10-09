/** Native Zen regression through its built-in WebDriver BiDi; uses an isolated temporary profile. */
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn } from 'node:child_process';
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
  throw new Error('Zen 条件未满足：' + expression);
}
function check(value,message) { if (!value) throw new Error(message); }
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
  await evaluate(`window.__zenTabs=document.querySelector('.admin-bar'); const start=document.startViewTransition.bind(document); document.startViewTransition=(...args)=>{ const transition=start(...args); window.__zenTransition=transition; transition.ready.then(()=>{ window.__zenSnapshot={group:getComputedStyle(document.documentElement,'::view-transition-group(page-content)').animationName,fit:getComputedStyle(document.documentElement,'::view-transition-new(page-content)').objectFit}; document.getAnimations().filter(a=>a.animationName==='page-arrive').forEach(a=>{a.pause();a.currentTime=275;}); }); return transition; }; true`);
  for (const target of ['media','themes']) {
    await evaluate(`window.__zenSnapshot=null; document.querySelector('.admin-bar a[href="/admin/${target}"]').click(); true`);
    await wait(`location.pathname==='/admin/${target}' && !!window.__zenSnapshot`);
    if (target==='themes') await wait(`!!document.querySelector('theme-workbench[data-bound=true]')`);
    check(await evaluate(`window.__zenTabs===document.querySelector('.admin-bar') && !document.querySelector('#content').contains(window.__zenTabs)`),'Zen 后台 Tab 被替换或进入正文动画');
    check(await evaluate(`window.__zenSnapshot.group==='none' && window.__zenSnapshot.fit==='contain'`),'Zen 快照仍有尺寸插值/拉伸');
    const screenshot = await command('browsingContext.captureScreenshot',{context,origin:'viewport'});
    await fs.writeFile(`/tmp/sloray-zen-${target}-transition.png`,Buffer.from(screenshot.data,'base64'));
    await evaluate('window.__zenTransition.skipTransition(); true');
  }
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
  console.log(`PASS Zen ${session.capabilities.browserVersion} 媒体/主题切页快照无拉伸、后台 Tab 保持节点、Toast 顶层显示/弹窗内可点击`);
  await command('session.end',{});
} finally {
  socket?.close(); processZen.kill('SIGTERM');
  await Promise.race([new Promise(resolve => processZen.once('exit',resolve)),delay(3000)]);
  if (processZen.exitCode===null) { processZen.kill('SIGKILL'); await delay(250); }
  await fs.rm(profile,{recursive:true,force:true});
}
