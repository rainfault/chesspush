import { Chessground } from './vendor/chessground.js';

const element = document.getElementById('board');
const promotion = document.getElementById('promotion');
// Decode every piece before the first interaction, including promotion pieces.
await Promise.all(['w', 'b'].flatMap(color => [...'KQRBNP'].map(role => {
  const image = new Image();
  image.src = `vendor/pieces/${color}${role}.svg`;
  return image.decode();
})));
let bridge, snapshot, waiting = false;
const send = data => bridge.dispatch(JSON.stringify(data));
const ground = Chessground(element, {
  coordinates: true, ranksPosition: 'right', disableContextMenu: true,
  animation: { enabled: true, duration: 200 },
  draggable: { enabled: true, showGhost: true },
  selectable: { enabled: true }, premovable: { enabled: false },
  movable: { free: false, dests: new Map(), events: { after: afterMove } },
  drawable: { enabled: true, visible: true, onChange: shapes => {
    if (bridge && snapshot) send({ action: 'shapes', shapes, revision: snapshot.revision });
  } },
});
function apply(raw) {
  snapshot = JSON.parse(raw);
  waiting = false;
  promotion.hidden = true;
  const config = { orientation: snapshot.orientation,
    turnColor: snapshot.turn, check: snapshot.check, lastMove: snapshot.lastMove || undefined,
    movable: { color: snapshot.turn, dests: new Map(Object.entries(snapshot.dests)) },
    drawable: { shapes: snapshot.shapes } };
  // A legal drop is already painted locally. Do not restart its animation when
  // Python acknowledges it. En passant, promotion and navigation still use FEN.
  if (ground.getFen() !== snapshot.fen.split(' ')[0]) config.fen = snapshot.fen;
  ground.set(config);
}
function afterMove(from, to) {
  if (waiting || !snapshot) return;
  waiting = true;
  ground.set({ movable: { dests: new Map() } });
  const submit = (piece = '') => send({ action: 'move', from, to, promotion: piece, revision: snapshot.revision });
  if (!snapshot.promotions.includes(from + to)) return submit();
  promotion.replaceChildren();
  const file = to.charCodeAt(0) - 97;
  const white = snapshot.orientation === 'white';
  const top = (to[1] === '8') === white;
  const choices = document.createElement('div');
  choices.className = 'choices';
  choices.style.left = `${(white ? file : 7 - file) * 12.5}%`;
  choices.style[top ? 'top' : 'bottom'] = '0';
  choices.style.flexDirection = top ? 'column' : 'column-reverse';
  for (const [piece, label] of [['q', 'Ферзь'], ['n', 'Конь'], ['r', 'Ладья'], ['b', 'Слон']]) {
    const button = document.createElement('button');
    button.setAttribute('aria-label', label);
    button.style.backgroundImage = `url(vendor/pieces/${snapshot.turn[0]}${piece.toUpperCase()}.svg)`;
    button.onclick = () => submit(piece);
    choices.append(button);
  }
  promotion.append(choices);
  promotion.hidden = false;
  choices.firstElementChild.focus();
}
promotion.addEventListener('click', event => {
  if (event.target === promotion) apply(JSON.stringify(snapshot));
});
document.addEventListener('keydown', event => {
  if (!snapshot) return;
  if (event.key === 'Escape') { apply(JSON.stringify(snapshot)); return; }
  if (!promotion.hidden) return;
  const cursors = { ArrowLeft: snapshot.cursor - 1, ArrowRight: snapshot.cursor + 1, Home: 0, End: snapshot.total };
  if (event.key in cursors) {
    event.preventDefault(); send({ action: 'seek', cursor: cursors[event.key] });
  } else if (event.key.toLowerCase() === 'f') send({ action: 'flip' });
});
new QWebChannel(qt.webChannelTransport, channel => {
  bridge = channel.objects.board;
  bridge.stateChanged.connect(() => apply(bridge.state));
  apply(bridge.state);
});
