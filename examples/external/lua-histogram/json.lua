-- json.lua -- minimal JSON codec for xtop external widgets.
-- Standard Lua only (5.1+/LuaJIT compatible: no bitwise operators, no goto).
--
-- json.null is the sentinel for JSON null. Decoded arrays/objects carry a
-- metatable so empty containers round-trip as [] / {}; json.array() and
-- json.object() let callers build tagged empty containers by hand.

local json = {}

json.null = setmetatable({}, { __tostring = function() return "null" end })

local array_mt = { __json_type = "array" }
local object_mt = { __json_type = "object" }

function json.array(t)
  return setmetatable(t or {}, array_mt)
end

function json.object(t)
  return setmetatable(t or {}, object_mt)
end

-- ---------------------------------------------------------------------------
-- Decoder
-- ---------------------------------------------------------------------------

local s, pos, len

local escapes = {
  ['"'] = '"', ['\\'] = '\\', ['/'] = '/',
  ['b'] = '\b', ['f'] = '\f', ['n'] = '\n', ['r'] = '\r', ['t'] = '\t',
}

local function fail(msg)
  error(("json: %s at byte %d"):format(msg, pos), 0)
end

local function skip_ws()
  while true do
    local c = s:byte(pos)
    if c == 32 or c == 9 or c == 10 or c == 13 then
      pos = pos + 1
    else
      return
    end
  end
end

-- Encode one Unicode code point as UTF-8 (no bitwise ops, LuaJIT-safe).
local function utf8(cp)
  if cp < 0x80 then
    return string.char(cp)
  elseif cp < 0x800 then
    return string.char(0xC0 + math.floor(cp / 0x40), 0x80 + cp % 0x40)
  elseif cp < 0x10000 then
    return string.char(
      0xE0 + math.floor(cp / 0x1000),
      0x80 + math.floor(cp / 0x40) % 0x40,
      0x80 + cp % 0x40
    )
  end
  return string.char(
    0xF0 + math.floor(cp / 0x40000),
    0x80 + math.floor(cp / 0x1000) % 0x40,
    0x80 + math.floor(cp / 0x40) % 0x40,
    0x80 + cp % 0x40
  )
end

local function parse_string()
  pos = pos + 1 -- opening quote
  local buf = {}
  while true do
    local c = s:byte(pos)
    if c == nil then
      fail("unterminated string")
    elseif c == 34 then -- closing quote
      pos = pos + 1
      return table.concat(buf)
    elseif c == 92 then -- backslash
      pos = pos + 1
      local e = s:sub(pos, pos)
      if e == 'u' then
        local hex = s:sub(pos + 1, pos + 4)
        if not hex:match("^%x%x%x%x$") then
          fail("bad \\u escape")
        end
        local cp = tonumber(hex, 16)
        pos = pos + 5
        -- Basic surrogate-pair support: high surrogate followed by \uDC00-\uDFFF.
        if cp >= 0xD800 and cp <= 0xDBFF and s:sub(pos, pos + 1) == "\\u" then
          local low_hex = s:sub(pos + 2, pos + 5)
          local low = low_hex:match("^%x%x%x%x$") and tonumber(low_hex, 16)
          if low and low >= 0xDC00 and low <= 0xDFFF then
            cp = 0x10000 + (cp - 0xD800) * 0x400 + (low - 0xDC00)
            pos = pos + 6
          end
        end
        buf[#buf + 1] = utf8(cp)
      else
        local mapped = escapes[e]
        if not mapped then
          fail("bad escape \\" .. tostring(e))
        end
        buf[#buf + 1] = mapped
        pos = pos + 1
      end
    elseif c < 0x20 then
      fail("control character in string")
    else
      buf[#buf + 1] = string.char(c)
      pos = pos + 1
    end
  end
end

local function parse_number()
  local start = pos
  if s:byte(pos) == 45 then -- '-'
    pos = pos + 1
  end
  if not s:sub(pos, pos):match("%d") then
    fail("invalid number")
  end
  while s:sub(pos, pos):match("%d") do
    pos = pos + 1
  end
  if s:sub(pos, pos) == "." then
    pos = pos + 1
    if not s:sub(pos, pos):match("%d") then
      fail("invalid number")
    end
    while s:sub(pos, pos):match("%d") do
      pos = pos + 1
    end
  end
  local e = s:sub(pos, pos)
  if e == "e" or e == "E" then
    pos = pos + 1
    local sign = s:sub(pos, pos)
    if sign == "+" or sign == "-" then
      pos = pos + 1
    end
    if not s:sub(pos, pos):match("%d") then
      fail("invalid exponent")
    end
    while s:sub(pos, pos):match("%d") do
      pos = pos + 1
    end
  end
  return tonumber(s:sub(start, pos - 1))
end

local parse_value, parse_array, parse_object

function parse_array()
  pos = pos + 1 -- '['
  local arr = setmetatable({}, array_mt)
  skip_ws()
  if s:byte(pos) == 93 then -- ']'
    pos = pos + 1
    return arr
  end
  while true do
    arr[#arr + 1] = parse_value()
    skip_ws()
    local c = s:byte(pos)
    if c == 44 then -- ','
      pos = pos + 1
    elseif c == 93 then -- ']'
      pos = pos + 1
      return arr
    else
      fail("expected ',' or ']'")
    end
  end
end

function parse_object()
  pos = pos + 1 -- '{'
  local obj = setmetatable({}, object_mt)
  skip_ws()
  if s:byte(pos) == 125 then -- '}'
    pos = pos + 1
    return obj
  end
  while true do
    skip_ws()
    if s:byte(pos) ~= 34 then
      fail("expected string key")
    end
    local key = parse_string()
    skip_ws()
    if s:byte(pos) ~= 58 then -- ':'
      fail("expected ':'")
    end
    pos = pos + 1
    obj[key] = parse_value()
    skip_ws()
    local c = s:byte(pos)
    if c == 44 then -- ','
      pos = pos + 1
    elseif c == 125 then -- '}'
      pos = pos + 1
      return obj
    else
      fail("expected ',' or '}'")
    end
  end
end

function parse_value()
  skip_ws()
  local c = s:byte(pos)
  if c == 123 then -- '{'
    return parse_object()
  elseif c == 91 then -- '['
    return parse_array()
  elseif c == 34 then -- '"'
    return parse_string()
  elseif c == 116 then -- 't'
    if s:sub(pos, pos + 3) == "true" then
      pos = pos + 4
      return true
    end
    fail("invalid literal")
  elseif c == 102 then -- 'f'
    if s:sub(pos, pos + 4) == "false" then
      pos = pos + 5
      return false
    end
    fail("invalid literal")
  elseif c == 110 then -- 'n'
    if s:sub(pos, pos + 3) == "null" then
      pos = pos + 4
      return json.null
    end
    fail("invalid literal")
  elseif c == 45 or (c and c >= 48 and c <= 57) then
    return parse_number()
  end
  fail("unexpected character")
end

function json.decode(text)
  if type(text) ~= "string" then
    error("json: decode expects a string", 0)
  end
  s, pos, len = text, 1, #text
  local value = parse_value()
  skip_ws()
  if pos <= len then
    fail("trailing garbage")
  end
  s = nil
  return value
end

-- ---------------------------------------------------------------------------
-- Encoder
-- ---------------------------------------------------------------------------

local escape_map = {
  ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f',
  ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t',
}

local function encode_string(value)
  local out = value:gsub('[%c"\\]', function(ch)
    return escape_map[ch] or ("\\u%04x"):format(ch:byte())
  end)
  return '"' .. out .. '"'
end

local function table_kind(t)
  local mt = getmetatable(t)
  if mt == array_mt then
    return "array"
  end
  if mt == object_mt then
    return "object"
  end
  local count, max, contiguous = 0, 0, true
  for k in pairs(t) do
    count = count + 1
    if type(k) ~= "number" or k < 1 or k % 1 ~= 0 then
      contiguous = false
    elseif k > max then
      max = k
    end
  end
  if count == 0 then
    return "object" -- ambiguous empty table: default to {}
  end
  if contiguous and max == count then
    return "array"
  end
  return "object"
end

local encode_value

local function encode_table(t)
  if t == json.null then
    return "null"
  end
  if table_kind(t) == "array" then
    local parts = {}
    for i = 1, #t do
      parts[i] = encode_value(t[i])
    end
    return "[" .. table.concat(parts, ",") .. "]"
  end
  local parts = {}
  for k, v in pairs(t) do
    if v ~= nil then
      local key = type(k) == "string" and k or tostring(k)
      parts[#parts + 1] = encode_string(key) .. ":" .. encode_value(v)
    end
  end
  return "{" .. table.concat(parts, ",") .. "}"
end

encode_value = function(v)
  if v == json.null then
    return "null"
  end
  local tv = type(v)
  if tv == "nil" then
    return "null"
  elseif tv == "boolean" then
    return v and "true" or "false"
  elseif tv == "number" then
    if v ~= v then
      error("json: cannot encode NaN", 0)
    end
    if v == math.huge or v == -math.huge then
      error("json: cannot encode infinity", 0)
    end
    if math.type and math.type(v) == "integer" then
      return string.format("%d", v)
    end
    -- %.14g keeps doubles stable for widget-scale values.
    return string.format("%.14g", v)
  elseif tv == "string" then
    return encode_string(v)
  elseif tv == "table" then
    return encode_table(v)
  end
  error("json: cannot encode " .. tv, 0)
end

function json.encode(value)
  return encode_value(value)
end

return json
