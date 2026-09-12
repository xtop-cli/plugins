-- lua-clock -- xtop external widget.
--
-- Speaks the xtop line-delimited JSON protocol on stdin/stdout: one JSON
-- request object per line in, one JSON response object per line out.
-- Standard Lua only; the codec lives in json.lua next to this file.

-- Resolve json.lua relative to this script so any cwd works.
local script = arg and arg[0] or "widget.lua"
local script_dir = script:match("^(.*[/\\])") or ""
if script_dir ~= "" then
  package.path = script_dir .. "?.lua;" .. package.path
end
local json = require("json")

local WIDGET_NAME = "lua-clock"
local VERSION = "0.1.0"

local COLOR_TITLE = { 200, 200, 200 }
local COLOR_CLOCK = { 123, 216, 143 }
local COLOR_DIM = { 140, 140, 140 }

local function send(obj)
  io.stdout:write(json.encode(obj), "\n")
  io.stdout:flush()
end

local function send_log(message)
  send({ type = "log", message = tostring(message) })
end

-- "3d 04:05:06" or "04:05:06" from uptime seconds.
local function format_uptime(seconds)
  seconds = tonumber(seconds) or 0
  local days = math.floor(seconds / 86400)
  local rest = seconds % 86400
  local hours = math.floor(rest / 3600)
  local minutes = math.floor((rest % 3600) / 60)
  local secs = rest % 60
  if days > 0 then
    return string.format("%dd %02d:%02d:%02d", days, hours, minutes, secs)
  end
  return string.format("%02d:%02d:%02d", hours, minutes, secs)
end

local function build_ops(state)
  local width = tonumber(state and state.width) or 0
  local height = tonumber(state and state.height) or 0
  if width <= 0 then
    width = 30
  end
  if height <= 0 then
    height = 6
  end

  local snapshot = (state and state.snapshot) or {}
  local unix_time = tonumber(state and state.unix_time) or 0
  local clock = os.date("!%H:%M:%S", unix_time) -- '!' = UTC
  local uptime = format_uptime(snapshot.uptime)
  local inner_width = math.max(width - 2, 1)
  local bottom = math.max(height - 2, 1)

  return {
    {
      op = "block",
      rect = { x = 0, y = 0, width = width, height = height },
      border = "rounded",
      title = WIDGET_NAME,
      fg = COLOR_TITLE,
      bg = json.null,
    },
    {
      op = "text",
      rect = { x = 1, y = 1, width = inner_width, height = 1 },
      spans = {
        { text = clock, fg = COLOR_CLOCK, bold = true },
      },
      align = "center",
      wrap = false,
    },
    {
      op = "text",
      rect = { x = 1, y = bottom, width = inner_width, height = 1 },
      spans = {
        { text = "uptime " .. uptime, fg = COLOR_DIM, dim = true },
      },
      align = "center",
      wrap = false,
    },
  }
end

-- Returns false when the guest should exit.
local function handle_request(request)
  local kind = request.type
  if kind == "manifest" then
    send({
      type = "manifest",
      manifest = {
        name = WIDGET_NAME,
        version = VERSION,
        description = "UTC clock and uptime rendered from the xtop state (Lua)",
        author = "xtop examples",
        max_processes = 1, -- this widget does not read processes
        api = "1",
      },
    })
  elseif kind == "render" then
    local ok, ops = pcall(build_ops, request.state)
    if not ok then
      send_log("render failed: " .. tostring(ops))
      send({ type = "draw", ops = {} })
    else
      send({ type = "draw", ops = ops })
    end
  elseif kind == "shutdown" then
    return false
  else
    send_log("unknown request type: " .. tostring(kind))
  end
  return true
end

-- EOF on stdin ends the loop cleanly.
for line in io.lines() do
  if line:match("%S") then
    local ok, request = pcall(json.decode, line)
    if not ok or type(request) ~= "table" then
      send_log("invalid JSON request: " .. tostring(request))
    elseif handle_request(request) == false then
      break
    end
  end
end

os.exit(0)
