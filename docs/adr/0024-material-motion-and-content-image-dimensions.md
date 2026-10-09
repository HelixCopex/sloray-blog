# ADR-0024：Material 动效与正文图片尺寸

- 状态：已接受
- 日期：2026-10-10
- Superseded by ADR-0025：第 3 项的图片尺寸读取失败处理；其余决策继续适用。
- 取代 ADR-0023 第 4 项中的缩放与无透明度切页动效；其余决策继续适用。

维护者要求切页取消缩放，上一内容滑出淡出、下一内容滑入淡入，并使用 Google
Material Design 3 的缓动曲线。新增 Dock 操作也需要缓动出现。图片在完全加载前
按原始比例占位，解码后由模糊转为清晰；宽高应作为文章内容属性随正文保存。

1. Astro ClientRouter 继续负责导航。只对 page-content 进行有前进/返回方向的位移
   与透明度动画：退出 200ms emphasized-accelerate `cubic-bezier(.3,0,.8,.15)`，
   进入 350ms emphasized-decelerate `cubic-bezier(.05,.7,.1,1)`，延迟 100ms，
   使交接短暂重叠。移除旧内容隐藏规则和正文缩放。
2. 持久 Dock 继续按稳定操作键保留节点及动画。仅新增的动作在首次可见时用
   250ms emphasized-decelerate 做 12px 上移和淡入；手机折叠时隐藏的新增动作
   等展开后出现。原有按钮不重新入场，旧浏览器的时间线恢复继续适用。
3. Markdown 图片使用 `![说明](<地址>){width=400 height=200}` 保存宽高，也支持
   引用式图片。该内容属性与 body_md 一起进入 Postgres、文章/页面修订及备份，
   不另建客户端权威缓存或数据库字段。媒体库复制时携带已知尺寸；保存编辑器时
   使用共用 Markdown AST 的精确偏移，浏览器读取缺失尺寸，再写入正文。
   代码块和转义文本不改写；外部图片读取失败时保留表单并提示修复地址或补全尺寸。
   服务器渲染不请求任意图片地址。
4. 清洗后的 Markdown 图片、响应式封面、媒体缩略图与 Dock favicon 使用原生
   focus-image 控件。有效宽高生成固定 aspect-ratio，图片 contain 完整显示。
   控件等待 load 与 decode 完成，才以 500ms emphasized-decelerate 从 24px 高斯
   模糊聚焦至零，并以 standard `cubic-bezier(.2,0,0,1)` 淡出色块。
   更换来源立即恢复占位，过期解码结果不更新当前图片；失败也保留几何尺寸。
5. 既有缺少尺寸的 Markdown 图片先使用稳定的 16:9 容器完整显示，编辑后保存
   即补齐原始比例。直接 API 写入者可使用同一内容属性；尺寸以正整数和有界
   像素数校验后生成样式，清洗流程不接受正文中的任意 style。
6. 减少动态效果偏好关闭切页、Dock 入场及图片模糊/渐变。禁用 JavaScript 时
   图片仍按 SSR 的宽高直接显示；动效是增强，不阻断阅读。持久 Dock 内已解码的
   favicon 在重新连接时保持就绪状态，避免切页触发重复聚焦。

不引入依赖、不修改数据库 schema。旧渲染器会把尺寸属性显示为普通文本，
回滚不会丢失正文或修订。以后需要更换富文本格式时，可由此明确的内容属性迁移。

曲线及时间 token 参考 Google 官方
[Material Components Motion](https://github.com/material-components/material-components-android/blob/master/docs/theming/Motion.md)。
