import {chromium} from '@playwright/test';
import {resolve} from 'node:path';
import {readFile} from 'node:fs/promises';
const browser=await chromium.connectOverCDP(process.env.CHESSPUSH_CDP??'http://127.0.0.1:9223');
const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(15000);
const errors=[];page.on('pageerror',e=>errors.push(e.message));
async function invoke(command,args={}){return page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});}
async function selectSource(path){await invoke('remember_file',{path:resolve(path)});await page.getByRole('button',{name:'Файлы',exact:true}).click();await page.getByRole('button',{name:`Выбрать ${path.split('/').pop()}`,exact:true}).click();}
try {
  await page.getByRole('button',{name:'Базы',exact:true}).click();
  await page.getByRole('button',{name:'Сбросить доску',exact:true}).click();
  await selectSource('tests/fixtures/corpus.pgn.zst');
  await page.getByText('Дополнительно',{exact:true}).click();
  await page.getByLabel('Минимум ходов',{exact:true}).fill('0');
  await page.getByRole('button',{name:'Найти партии',exact:true}).click();
  await page.locator('.result-status').filter({hasText:'Готово'}).waitFor();
  const first=await invoke('library_files');
  const exported=first.find(f=>f.kind==='export');if(!exported||exported.games!==7)throw new Error(JSON.stringify(first));
  const text=await readFile(exported.path,'utf8');if((text.match(/\[Event /g)??[]).length!==7)throw new Error('PGN parity failed');
  await selectSource('.qa/benchmark.pgn.zst');
  await page.getByLabel('Количество',{exact:true}).fill('1000000');
  await page.getByRole('button',{name:'Найти партии',exact:true}).click();
  await page.waitForFunction(()=>Number(document.querySelector('.counts strong')?.textContent?.replace(/\D/g,''))>0);
  // The board and navigation stay interactive while Rust is reading the dump.
  await page.getByRole('button',{name:'Перевернуть доску',exact:true}).click();
  await page.getByRole('button',{name:'Остановить',exact:true}).click();
  await page.locator('.result-status').filter({hasText:'Остановлено'}).waitFor();
  const cancelled=await invoke('library_files');const partial=cancelled.find(f=>f.kind==='export');
  if(!partial || partial.games<1)throw new Error('Cancelled records were not persisted');
  await selectSource('tests/fixtures/truncated.pgn.zst');
  await page.getByRole('button',{name:'Найти партии',exact:true}).click();
  await page.getByRole('alert').waitFor();
  await page.getByRole('button',{name:'Закрыть ошибку',exact:true}).click();
  await page.getByRole('button',{name:'Настройки',exact:true}).click();
  await page.getByLabel('Тема',{exact:true}).selectOption('dark');
  await page.getByRole('button',{name:'Закрыть настройки',exact:true}).click();
  if(await invoke('get_theme')!=='dark')throw new Error('Theme persistence failed');
  await page.reload();await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark');
  await page.getByRole('button',{name:'Файлы',exact:true}).click();await page.locator('.file-row').first().waitFor();
  await page.screenshot({path:'.qa/native.png'});
  if(errors.length)throw new Error(errors.join('\n'));
  console.log('PASS: native commands/events, 7 exact PGN matches, progress, cancellation, responsive board, corrupt-input error, SQLite file history and theme reload');
} catch(e) {await page.screenshot({path:'.qa/native-failure.png'});throw e;}
finally {await browser.close();}
