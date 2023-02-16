export class Point {
  x: number;
  y: number;

  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

  index(): number {
    let x = zigzag_encode(this.x);
    let y = zigzag_encode(this.y);
    return interleave(x, y);
  }

  static from_index(i: number): Point {
    let [x, y] = deinterleave(i);
    return new Point(zigzag_decode(x), zigzag_decode(y));
  }

  indexes(): [number, number, number] {
    let i = this.index();
    let slot_i, word_i;
    [i, slot_i] = extract_lo_bits(i, SLOT_INDEX_BITS);
    [i, word_i] = extract_lo_bits(i, WORD_INDEX_BITS);
    return [i, word_i, slot_i];
  }
}

function extract_lo_bits(i: number, bits: number): [number, number] {
  return [i >>> bits, i & ((1 << bits) - 1)];
}

function zigzag_encode(x: number): number {
  return ((x << 1) ^ (x >> 31)) >>> 0;
}

function zigzag_decode(x: number): number {
  return ((x >>> 1) ^ -(x & 1)) >> 0;
}

function scatter(x: number): number {
  x = (x | (x << 8)) & 0x00ff00ff;
  x = (x | (x << 4)) & 0x0f0f0f0f;
  x = (x | (x << 2)) & 0x33333333;
  return (x | (x << 1)) & 0x55555555;
}

function interleave(x: number, y: number): number {
  return scatter(x) | (scatter(y) << 1);
}

function gather(x: number): number {
  x &= 0x55555555;
  x = (x | (x >>> 1)) & 0x33333333;
  x = (x | (x >>> 2)) & 0x0f0f0f0f;
  x = (x | (x >>> 4)) & 0x00ff00ff;
  return (x | (x >>> 8)) & 0x0000ffff;
}

function deinterleave(x: number): [number, number] {
  return [gather(x), gather(x >>> 1)];
}

export enum Stone {
  Black = 0,
  White = 1,
}

const CHUNK_SIZE_BITS = 4;
const CHUNK_SIZE = 1 << CHUNK_SIZE_BITS;
const WORDS_PER_CHUNK = CHUNK_SIZE * CHUNK_SIZE * 2 / 32;

const SLOT_INDEX_BITS = 4;
const WORD_INDEX_BITS = CHUNK_SIZE_BITS * 2 - SLOT_INDEX_BITS;

class Chunk {
  private words: Uint32Array;

  constructor() {
    this.words = new Uint32Array(WORDS_PER_CHUNK);
  }

  get(word_i: number, slot_i: number): Stone | null {
    let word = this.words[word_i];
    switch ((word >>> (slot_i * 2)) & 3) {
      case 0:
        return null;
      case 1:
        return Stone.Black;
    }
    return Stone.White;
  }

  set(word_i: number, slot_i: number, stone: Stone): boolean {
    let word = this.words[word_i];
    if (((word >>> (slot_i * 2)) & 3) != 0) {
      return false;
    }
    this.words[word_i] = word | (1 << (slot_i * 2 + stone));
    return true;
  }

  unset(word_i: number, slot_i: number): Stone | null {
    let word = this.words[word_i];
    this.words[word_i] = word & ~(3 << (slot_i * 2));
    switch ((word >>> (slot_i * 2)) & 3) {
      case 0:
        return null;
      case 1:
        return Stone.Black;
    }
    return Stone.White;
  }
}

class RawBoard {
  private chunks: Map<number, Chunk>;

  constructor() {
    this.chunks = new Map();
  }

  get(point: Point): Stone | null {
    let [chunk_i, word_i, slot_i] = point.indexes();
    let chunk = this.chunk(chunk_i);
    if (chunk == undefined) {
      return null;
    }
    return chunk.get(word_i, slot_i);
  }

  set(point: Point, stone: Stone): boolean {
    let [chunk_i, word_i, slot_i] = point.indexes();
    return this.chunk_or_default(chunk_i).set(word_i, slot_i, stone);
  }

  unset(point: Point): Stone | null {
    let [chunk_i, word_i, slot_i] = point.indexes();
    let chunk = this.chunk(chunk_i);
    if (chunk == undefined) {
      return null;
    }
    return chunk.unset(word_i, slot_i);
  }

  chunk(chunk_i: number): Chunk | undefined {
    return this.chunks.get(chunk_i);
  }

  chunk_or_default(chunk_i: number): Chunk {
    let chunk = this.chunks.get(chunk_i);
    if (chunk == undefined) {
      chunk = new Chunk();
      this.chunks.set(chunk_i, chunk);
    }
    return chunk;
  }
}

export class Board {
  private board: RawBoard;
  private record: [Point, Stone][];
  private _index: number;

  constructor() {
    this.board = new RawBoard();
    this.record = [];
    this._index = 0;
  }

  static from_record(record: [{ x: number, y: number; }, Stone][]): Board {
    let board = new Board();
    record.forEach(move => {
      board.set(new Point(move[0].x, move[0].y), move[1]);
    });
    return board;
  }

  total_count(): number {
    return this.record.length;
  }

  index(): number {
    return this._index;
  }

  is_empty(): boolean {
    return this._index == 0;
  }

  get(point: Point): Stone | null {
    return this.board.get(point);
  }

  past_record(): [Point, Stone][] {
    return this.record.slice(0, this._index);
  }

  set(point: Point, stone: Stone): boolean {
    if (!this.board.set(point, stone)) {
      return false;
    }
    this.record.splice(this._index);
    this.record.push([point, stone]);
    this._index += 1;
    return true;
  }

  unset(): [Point, Stone] | null {
    if (this._index == 0) {
      return null;
    }
    this._index -= 1;
    let last = this.record[this._index];

    this.board.unset(last[0]);
    return last;
  }

  reset(): [Point, Stone] | null {
    if (this._index >= this.record.length) {
      return null;
    }
    let next = this.record[this._index];
    this._index += 1;

    this.board.set(next[0], next[1]);
    return next;
  }

  jump(index: number) {
    if (index > this.record.length) {
      return;
    }
    if (this._index < index) {
      for (let i = this._index; i < index; i++) {
        let next = this.record[i];
        this.board.set(next[0], next[1]);
      }
    } else {
      for (let i = this._index - 1; i >= index; i--) {
        let last = this.record[i];
        this.board.unset(last[0]);
      }
    }
    this._index = index;
  }

  infer_turn(): [Stone, boolean] {
    if (this._index == 0) return [Stone.Black, true];

    let last = this.record[this._index - 1][1];
    if (this._index == 1) return [Stone.White, last == Stone.White];

    let last_prev = this.record[this._index - 2][1];
    if (last == last_prev) return [last ^ 1, false];
    return [last, true];
  }
}
