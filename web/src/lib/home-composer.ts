import { homeRows, widgetLabels, defaultTheme } from './theme.mjs';
import type { components } from './schema';
type Row = components['schemas']['HomeRow'] & { widgets: components['schemas']['HomeWidget'][] };
/** Controlled rows and widgets; never accepts executable templates. */
export function homeComposer(form: HTMLFormElement) {
  const input = form.elements.namedItem('home_rows') as HTMLTextAreaElement;
  const blocks = form.elements.namedItem('home_blocks') as HTMLInputElement;
  const host = form.querySelector<HTMLElement>('[data-home-composer]')!;
  let rows: Row[] = [];
  const field = (value: string, label: string, name: string, row: number, widget?: number) => {
    const wrapper = document.createElement('label'); wrapper.textContent = label;
    const control = document.createElement(name === 'text' ? 'textarea' : 'input');
    control.value = value; control.dataset.row = String(row); control.dataset.field = name;
    control.maxLength = name === 'text' ? 1000 : 100;
    if (widget !== undefined) control.dataset.widget = String(widget);
    wrapper.append(control); return wrapper;
  };
  const button = (title: string, action: string, row: number, widget?: number) => {
    const el = document.createElement('button'); el.type = 'button'; el.textContent = title;
    el.dataset.action = action; el.dataset.row = String(row);
    if (widget !== undefined) el.dataset.widget = String(widget);
    return el;
  };
  const write = () => { input.value = JSON.stringify(rows); input.dispatchEvent(new Event('input', { bubbles: true })); };
  const render = () => {
    host.replaceChildren();
    rows.forEach((row, i) => {
      const card = document.createElement('fieldset'); card.className = 'composer-row';
      const legend = document.createElement('legend'); legend.textContent = `${i + 1}. ${{ hero: '引言', posts: '文章列表', widgets: '小组件域' }[row.kind] ?? '页面目录'}`; card.append(legend);
      const actions = document.createElement('div'); actions.className = 'actions';
      const up = button('上移', 'up', i); up.disabled = i === 0;
      const down = button('下移', 'down', i); down.disabled = i === rows.length - 1;
      actions.append(up, down);
      if (row.kind !== 'posts') actions.append(button('移除区块', 'remove', i));
      card.append(actions, field(row.title ?? '', '区块标题（可选）', 'title', i));
      row.widgets.forEach((widget, wi) => {
        const area = document.createElement('div'); area.className = 'composer-widget';
        const label = document.createElement('label'); label.textContent = '小组件类型';
        const select = document.createElement('select'); select.dataset.row = String(i); select.dataset.widget = String(wi); select.dataset.field = 'kind';
        Object.entries(widgetLabels).forEach(([value, text]) => { const option = document.createElement('option'); option.value = value; option.textContent = text; select.append(option); }); select.value = widget.kind;
        label.append(select); area.append(label, field(widget.title ?? '', '小组件标题', 'title', i, wi));
        if (['note','subscribe'].includes(widget.kind)) area.append(field(widget.text ?? '', '文字内容', 'text', i, wi));
        const move = button('向前', 'widget-up', i, wi); move.disabled = wi === 0;
        area.append(move, button('移除组件', 'widget-remove', i, wi)); card.append(area);
      });
      if (row.kind === 'widgets' && row.widgets.length < 4) card.append(button('添加小组件', 'widget-add', i));
      host.append(card);
    });
  };
  host.addEventListener('input', event => {
    const target = event.target as HTMLInputElement;
    const row = rows[Number(target.dataset.row)]; if (!row || !target.dataset.field) return;
    if (target.dataset.widget === undefined) row.title = target.value;
    else {
      const widget = row.widgets[Number(target.dataset.widget)];
      const key = target.dataset.field as 'kind' | 'title' | 'text'; widget[key] = target.value;
      if (key === 'kind') render();
    }
    write();
  });
  host.addEventListener('click', event => {
    const target = (event.target as Element).closest<HTMLButtonElement>('[data-action]'); if (!target) return;
    const i = Number(target.dataset.row), wi = Number(target.dataset.widget), row = rows[i];
    switch (target.dataset.action) {
      case 'up': if (i > 0) [rows[i-1], rows[i]] = [row, rows[i-1]]; break;
      case 'down': if (i < rows.length-1) [rows[i], rows[i+1]] = [rows[i+1], row]; break;
      case 'remove': if (row.kind !== 'posts') rows.splice(i, 1); break;
      case 'widget-add': row.widgets.push({ kind: 'note', title: '', text: '' }); break;
      case 'widget-remove': row.widgets.splice(wi, 1); if (!row.widgets.length) rows.splice(i, 1); break;
      case 'widget-up': if (wi > 0) [row.widgets[wi-1], row.widgets[wi]] = [row.widgets[wi], row.widgets[wi-1]]; break;
    }
    write(); render();
  });
  form.querySelectorAll<HTMLButtonElement>('[data-add-row]').forEach(control => control.addEventListener('click', () => {
    if (rows.length >= 12) return;
    const kind = control.dataset.addRow!;
    if (kind === 'hero' && rows.some(row => row.kind === 'hero')) return;
    rows.push({ kind, title: '', widgets: kind === 'widgets' ? [{ kind: 'note', title: '', text: '' }] : [] }); write(); render();
  }));
  const load = () => {
    const stored = JSON.parse(input.value || '[]') as Row[];
    rows = stored.length ? stored : homeRows({ ...defaultTheme, layout: { ...defaultTheme.layout, home_blocks: blocks.value.split(',').map(v => v.trim()).filter(Boolean) } }).map(row => row.kind === 'pages' ? {kind: 'widgets', title: '页面', widgets: [{kind: 'pages',title: '',text: ''}]} : row);
    render();
  };
  blocks.addEventListener('input', () => { input.value = '[]'; load(); });
  return load;
}
