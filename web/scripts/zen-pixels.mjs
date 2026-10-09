/** Read native screenshots with Node's zlib; no image library/runtime dependency. */
import { inflateSync } from 'node:zlib';
export function landmarkBounds(buffer, kind) {
  const chunks = []; let width, height, channels;
  for (let at = 8; at < buffer.length;) {
    const length = buffer.readUInt32BE(at), type = buffer.toString('ascii',at+4,at+8), body = buffer.subarray(at+8,at+8+length);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0); height = body.readUInt32BE(4);
      if (body[8] !== 8 || ![2,6].includes(body[9]) || body[12] !== 0) throw new Error('Zen screenshot must be noninterlaced RGB/RGBA PNG');
      channels = body[9] === 6 ? 4 : 3;
    }
    if (type === 'IDAT') chunks.push(body);
    at += length + 12;
  }
  const raw = inflateSync(Buffer.concat(chunks)), stride = width * channels;
  let previous = Buffer.alloc(stride), minX = width, maxX = -1, minY = height, maxY = -1, count = 0, red = 0, green = 0, blue = 0;
  const paeth = (a,b,c) => { const p=a+b-c, x=Math.abs(p-a), y=Math.abs(p-b), z=Math.abs(p-c); return x<=y && x<=z ? a : y<=z ? b : c; };
  for (let y=0, at=0; y<height; y++) {
    const filter=raw[at++], row=Buffer.alloc(stride);
    for (let x=0;x<stride;x++) {
      const left=x>=channels?row[x-channels]:0, above=previous[x], corner=x>=channels?previous[x-channels]:0;
      row[x]=(raw[at++]+(filter===0?0:filter===1?left:filter===2?above:filter===3?Math.floor((left+above)/2):paeth(left,above,corner)))&255;
    }
    for (let x=112;x<width;x++) {
      const [r,g,b]=row.subarray(x*channels,x*channels+3);
      const matches=kind==='old' ? r>g+15 && b>g+10 && r>b+10 : g>r+20 && b>r+15 && g>b+5;
      if (matches) { minX=Math.min(minX,x); maxX=Math.max(maxX,x); minY=Math.min(minY,y); maxY=Math.max(maxY,y); count++; red+=r; green+=g; blue+=b; }
    }
    previous=row;
  }
  return {width:maxX<0?0:maxX-minX+1,height:maxY<0?0:maxY-minY+1,count,x:minX,y:minY,rgb:count?[red/count,green/count,blue/count].map(Math.round):null};
}
