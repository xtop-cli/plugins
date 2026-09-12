-- lua-histogram -- xtop external widget.
--
-- Statistical CPU usage histogram in Lua. Keeps a rolling history of the
-- per-tick average, bins it into 8 frequency buckets over 0-100% and draws
-- one `bar` op per bin (the host's bar is a single-line gauge, so stacking
-- them vertically yields a histogram). A footer shows mean, standard
-- deviation and p95.
--
-- Standard Lua only; the JSON codec lives in json.lua next to this file.

-- Resolve json.lua relative to this script so any cwd works.
local script = arg and arg[0] or "widget.lua"
local script_dir = script:match("^(.*[/\\])") or ""
if script_dir ~= "" then
  package.path = script_dir .. "?.lua;" .. package.path
end
local json = require("json")

local WIDGET_NAME = "lua-histogram"
local VERSION = "0.1.0"
local HISTORY_LEN = 240
local BIN_COUNT = 8

local COLOR_TITLE = { 200, 200, 200 }
local COLOR_DIM = { 140, 140, 140 }
local COLOR_GREEN = { 123, 216, 143 }
local COLOR_YELLOW = { 255, 200, 64 }
local COLOR_RED = { 255, 85, 85 }
local COLOR_CURRENT = { 90, 212, 230 }

local history = {}

local function send(obj)
  io.stdout:write(json.encode(obj), "\n")
  io.stdout:flush()
end

local function send_log(message)
  send({ type = "log", message = tostring(message) })
end

local function bin_color(lo)
  if lo >= 75 then
    return COLOR_RED
  elseif lo >= 50 then
    return COLOR_YELLOW
  end
  return COLOR_GREEN
end

local function mean(values)
  local total = 0
  for _, value in ipairs(values) do
    total = total + value
  end
  return total / #values
end

local function stdev(values, avg)
  local total = 0
  for _, value in ipairs(values) do
    local delta = value - avg
    total = total + delta * delta
  end
  return math.sqrt(total / #values)
end

local function percentile(values, fraction)
  local ordered = {}
  for i, value in ipairs(values) do
    ordered[i] = value
  end
  table.sort(ordered)
  local index = math.ceil(fraction * #ordered)
  if index < 1 then
    index = 1
  end
  if index > #ordered then
    index = #ordered
  end
  return ordered[index]
end

local function build_ops(state)
  local width = tonumber(state and state.width) or 0
  local height = tonumber(state and state.height) or 0
  if width <= 0 then
    width = 40
  end
  if height <= 0 then
    height = 12
  end
  local inner = math.max(width - 2, 1)

  local snapshot = (state and state.snapshot) or {}
  local cpus = snapshot.cpus or {}
  local usage = 0
  if #cpus > 0 then
    for _, cpu in ipairs(cpus) do
      usage = usage + (tonumber(cpu.usage) or 0)
    end
    usage = usage / #cpus
  end
  usage = math.max(0, math.min(100, usage))

  history[#history + 1] = usage
  if #history > HISTORY_LEN then
    table.remove(history, 1)
  end

  local ops = {
    {
      op = "block",
      rect = { x = 0, y = 0, width = width, height = height },
      border = "rounded",
      title = WIDGET_NAME,
      fg = COLOR_TITLE,
      bg = nil,
    },
  }

  if width < 18 or height < 6 then
    return ops
  end

  if #history < 2 then
    ops[#ops + 1] = {
      op = "text",
      rect = { x = 1, y = 2, width = inner, height = 1 },
      spans = { { text = "collecting samples...", fg = COLOR_DIM, dim = true } },
      align = "center",
      wrap = false,
    }
    return ops
  end

  -- Frequency bins over 0-100%.
  local counts = {}
  for i = 1, BIN_COUNT do
    counts[i] = 0
  end
  local max_count = 0
  local current_bin = 1
  for _, value in ipairs(history) do
    local index = math.floor(value / (100 / BIN_COUNT)) + 1
    if index > BIN_COUNT then
      index = BIN_COUNT
    end
    counts[index] = counts[index] + 1
    if counts[index] > max_count then
      max_count = counts[index]
    end
    if value == usage then
      current_bin = index
    end
  end
  if max_count < 1 then
    max_count = 1
  end

  local rows = math.min(BIN_COUNT, math.max(height - 3, 1))
  local bin_width = 100 / BIN_COUNT
  for i = 1, rows do
    local lo = math.floor((i - 1) * bin_width)
    local hi = math.floor(i * bin_width)
    local color = bin_color(lo)
    if i == current_bin then
      color = COLOR_CURRENT
    end
    ops[#ops + 1] = {
      op = "bar",
      rect = { x = 1, y = i, width = inner, height = 1 },
      ratio = counts[i] / max_count,
      label = string.format("%3d-%3d%% %4d", lo, hi, counts[i]),
      fg = color,
      bg = nil,
      border = nil,
    }
  end

  local avg = mean(history)
  local sigma = stdev(history, avg)
  local footer = string.format(
    "n=%d  mean %.1f  sigma %.1f  p95 %.0f",
    #history,
    avg,
    sigma,
    percentile(history, 0.95)
  )
  ops[#ops + 1] = {
    op = "text",
    rect = { x = 1, y = math.max(height - 2, 1), width = inner, height = 1 },
    spans = { { text = footer, fg = COLOR_DIM, dim = true } },
    align = "center",
    wrap = false,
  }
  return ops
end

local function handle(request)
  local kind = request and request.type
  if kind == "manifest" then
    send({
      type = "manifest",
      manifest = {
        name = WIDGET_NAME,
        version = VERSION,
        description = "CPU usage histogram with mean/sigma/p95 (Lua)",
        author = "xtop examples",
        max_processes = 1,
        api = "1",
      },
    })
  elseif kind == "render" then
    local ok, ops = pcall(build_ops, request.state)
    if not ok then
      send_log("render failed: " .. tostring(ops))
      ops = {}
    end
    send({ type = "draw", ops = ops })
  elseif kind == "shutdown" then
    return false
  else
    send_log("unknown request type: " .. tostring(kind))
  end
  return true
end

for line in io.lines() do
  if line:match("%S") then
    local ok, request = pcall(json.decode, line)
    if not ok or type(request) ~= "table" then
      send_log("invalid JSON request")
    elseif not handle(request) then
      break
    end
  end
end
