#!/usr/bin/env node
// node-clock -- xtop external widget.
//
// Speaks the xtop line-delimited JSON protocol on stdin/stdout: one JSON
// request object per line in, one JSON response object per line out.
// Node.js standard library only.

'use strict';

const readline = require('readline');

const WIDGET_NAME = 'node-clock';
const VERSION = '0.1.0';

const COLOR_TITLE = [200, 200, 200];
const COLOR_CLOCK = [123, 216, 143];
const COLOR_DIM = [140, 140, 140];

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function sendLog(message) {
  send({ type: 'log', message: String(message) });
}

// "3d 04:05:06" or "04:05:06" from uptime seconds.
function formatUptime(seconds) {
  const total = Math.max(0, Math.floor(Number(seconds) || 0));
  const days = Math.floor(total / 86400);
  const rest = total % 86400;
  const pad = (n) => String(n).padStart(2, '0');
  const hms = `${pad(Math.floor(rest / 3600))}:${pad(Math.floor((rest % 3600) / 60))}:${pad(rest % 60)}`;
  return days > 0 ? `${days}d ${hms}` : hms;
}

function utcClock(unixTime) {
  return new Date((Number(unixTime) || 0) * 1000).toISOString().slice(11, 19);
}

function buildOps(state) {
  if (state === null || typeof state !== 'object') state = {};
  const snapshot =
    state.snapshot !== null && typeof state.snapshot === 'object' ? state.snapshot : {};

  const width = Number(state.width) > 0 ? Number(state.width) : 30;
  const height = Number(state.height) > 0 ? Number(state.height) : 6;
  const clock = utcClock(state.unix_time);
  const uptime = formatUptime(snapshot.uptime);
  const innerWidth = Math.max(width - 2, 1);
  const bottom = Math.max(height - 2, 1);

  return [
    {
      op: 'block',
      rect: { x: 0, y: 0, width, height },
      border: 'rounded',
      title: WIDGET_NAME,
      fg: COLOR_TITLE,
      bg: null,
    },
    {
      op: 'text',
      rect: { x: 1, y: 1, width: innerWidth, height: 1 },
      spans: [{ text: clock, fg: COLOR_CLOCK, bold: true }],
      align: 'center',
      wrap: false,
    },
    {
      op: 'text',
      rect: { x: 1, y: bottom, width: innerWidth, height: 1 },
      spans: [{ text: 'uptime ' + uptime, fg: COLOR_DIM, dim: true }],
      align: 'center',
      wrap: false,
    },
  ];
}

// Returns false when the guest should exit.
function handle(request) {
  const kind = request.type;
  if (kind === 'manifest') {
    send({
      type: 'manifest',
      manifest: {
        name: WIDGET_NAME,
        version: VERSION,
        description: 'UTC clock and uptime rendered from the xtop state (Node.js)',
        author: 'xtop examples',
        max_processes: 1, // this widget does not read processes
        api: '1',
      },
    });
  } else if (kind === 'render') {
    let ops = [];
    try {
      ops = buildOps(request.state);
    } catch (err) {
      sendLog('render failed: ' + err.message);
      ops = [];
    }
    send({ type: 'draw', ops });
  } else if (kind === 'shutdown') {
    return false;
  } else {
    sendLog('unknown request type: ' + kind);
  }
  return true;
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on('line', (line) => {
  if (!line.trim()) return; // ignore blank lines
  let request;
  try {
    request = JSON.parse(line);
  } catch (err) {
    sendLog('invalid JSON request: ' + err.message);
    return;
  }
  if (request === null || typeof request !== 'object' || Array.isArray(request)) {
    sendLog('request is not a JSON object');
    return;
  }
  if (!handle(request)) rl.close();
});

// Fires on shutdown and on stdin EOF; both exit 0.
rl.on('close', () => process.exit(0));
