-- ZoneTwo: play WoW with Zwift Ride controllers (the zonetwo program sends the keys).
-- /zt on       turn controller mode on (temporary override bindings + soft targeting)
-- /zt off      turn it off: your normal bindings and settings are back
-- /zt autorun  toggle: Up = autorun toggle instead of hold-to-move
-- /zt hud      show/hide the button cheat-sheet
--
-- Bindings are override bindings: WoW never saves them, so a crash or disconnect
-- can't change your real key bindings.

local ADDON = ...
local PREFIX = "|cff33ff99ZoneTwo|r: "

-- Ride button -> key the zonetwo program sends -> WoW binding command
local MAP = {
    { button = "Up",          key = "W",      cmd = "MOVEFORWARD",        label = "Forward" },
    { button = "Down",        key = "S",      cmd = "MOVEBACKWARD",       label = "Back" },
    { button = "Left",        key = "A",      cmd = "TURNLEFT",           label = "Turn left" },
    { button = "Right",       key = "D",      cmd = "TURNRIGHT",          label = "Turn right" },
    { button = "A",           key = "1",      cmd = "ACTIONBUTTON1",      label = "Bar 1" },
    { button = "B",           key = "2",      cmd = "ACTIONBUTTON2",      label = "Bar 2" },
    { button = "Y",           key = "3",      cmd = "ACTIONBUTTON3",      label = "Bar 3" },
    { button = "Z",           key = "4",      cmd = "ACTIONBUTTON4",      label = "Bar 4" },
    { button = "R shift +",   key = "5",      cmd = "ACTIONBUTTON5",      label = "Bar 5" },
    { button = "R shift -",   key = "6",      cmd = "ACTIONBUTTON6",      label = "Bar 6" },
    { button = "L shift +",   key = "TAB",    cmd = "TARGETNEARESTENEMY", label = "Next enemy" },
    { button = "L shift -",   key = "ESCAPE", cmd = nil,                  label = "Clear / close" }, -- default Esc behaviour
    { button = "R paddle",    key = "SPACE",  cmd = "JUMP",               label = "Jump" },
    { button = "L paddle",    key = "F",      cmd = "INTERACTTARGET",     label = "Interact" },
    { button = "R power-up",  key = "M",      cmd = "TOGGLEWORLDMAP",     label = "Map" },
    { button = "L power-up",  key = "7",      cmd = "ACTIONBUTTON7",      label = "Bar 7 (mount)" },
}

-- Gamepad-style feel. Values: soft target 3 = always.
local CVARS = {
    SoftTargetEnemy     = "3",
    SoftTargetFriend    = "0",
    SoftTargetInteract  = "3",
    SoftTargetForce     = "1",
    SoftTargetIconEnemy = "1",
    SoftTargetIconInteract = "1",
    autoLootDefault     = "1",
    autoInteract        = "0", -- click-to-move off
}

local function say(msg) DEFAULT_CHAT_FRAME:AddMessage(PREFIX .. msg) end

local function getCVar(name)
    if C_CVar and C_CVar.GetCVar then return C_CVar.GetCVar(name) end
    return GetCVar(name)
end

local function setCVar(name, value)
    if C_CVar and C_CVar.SetCVar then return C_CVar.SetCVar(name, value) end
    return SetCVar(name, value)
end

local owner = CreateFrame("Frame")
local pendingAfterCombat

local function upCommand()
    return ZoneTwoDB.autorun and "TOGGLEAUTORUN" or "MOVEFORWARD"
end

local function setOverrides()
    ClearOverrideBindings(owner)
    for _, m in ipairs(MAP) do
        local cmd = m.button == "Up" and upCommand() or m.cmd
        if cmd then SetOverrideBinding(owner, true, m.key, cmd) end
    end
end

local function applyCVars()
    ZoneTwoDB.cvarBackup = ZoneTwoDB.cvarBackup or {}
    local missing = {}
    for name, value in pairs(CVARS) do
        local current = getCVar(name)
        if current == nil then
            missing[#missing + 1] = name
        else
            if ZoneTwoDB.cvarBackup[name] == nil then ZoneTwoDB.cvarBackup[name] = current end
            setCVar(name, value)
        end
    end
    return missing
end

local function restoreCVars()
    for name, value in pairs(ZoneTwoDB.cvarBackup or {}) do setCVar(name, value) end
    ZoneTwoDB.cvarBackup = nil
end

-- Versions before 0.2 changed the saved bindings directly. Put them back once.
local function migrateLegacySnapshot()
    local snap = ZoneTwoDB.snapshot
    if not snap then return end
    for key, action in pairs(snap.bindings or {}) do
        if action == "" then SetBinding(key) else SetBinding(key, action) end
    end
    SaveBindings(GetCurrentBindingSet and GetCurrentBindingSet() or 1)
    for name, value in pairs(snap.cvars or {}) do setCVar(name, value) end
    ZoneTwoDB.snapshot, ZoneTwoDB.applied = nil, nil
    say("restored the key bindings an older ZoneTwo version had changed.")
end

local function runOutOfCombat(fn)
    if InCombatLockdown() then
        pendingAfterCombat = fn
        owner:RegisterEvent("PLAYER_REGEN_ENABLED")
        say("in combat; will switch after combat.")
    else
        fn()
    end
end

local function turnOn(quiet)
    runOutOfCombat(function()
        setOverrides()
        local missing = applyCVars()
        ZoneTwoDB.enabled = true
        if not quiet then
            say("controller mode ON. /zt off for your normal keys.")
            if #missing > 0 then say("not in this client (skipped): " .. table.concat(missing, ", ")) end
        end
    end)
end

local function turnOff()
    runOutOfCombat(function()
        ClearOverrideBindings(owner)
        restoreCVars()
        ZoneTwoDB.enabled = false
        say("controller mode OFF. Your normal keys are back.")
    end)
end

-- HUD -----------------------------------------------------------------------
local hud

local function hudText()
    local lines = {}
    for _, m in ipairs(MAP) do
        local label = m.label
        if m.button == "Up" and ZoneTwoDB.autorun then label = "Autorun" end
        lines[#lines + 1] = ("|cffffd100%-10s|r %s"):format(m.button, label)
    end
    return table.concat(lines, "\n")
end

local function buildHud()
    hud = CreateFrame("Frame", "ZoneTwoHUD", UIParent, BackdropTemplateMixin and "BackdropTemplate" or nil)
    hud:SetSize(190, 16 * #MAP + 16)
    hud:SetPoint(unpack(ZoneTwoDB.hudPoint or { "RIGHT", UIParent, "RIGHT", -20, 0 }))
    if hud.SetBackdrop then
        hud:SetBackdrop({ bgFile = "Interface\\Tooltips\\UI-Tooltip-Background",
                          edgeFile = "Interface\\Tooltips\\UI-Tooltip-Border",
                          edgeSize = 12, insets = { left = 3, right = 3, top = 3, bottom = 3 } })
        hud:SetBackdropColor(0, 0, 0, 0.6)
    end
    hud:SetMovable(true)
    hud:EnableMouse(true)
    hud:RegisterForDrag("LeftButton")
    hud:SetScript("OnDragStart", hud.StartMoving)
    hud:SetScript("OnDragStop", function(self)
        self:StopMovingOrSizing()
        local p, _, rp, x, y = self:GetPoint()
        ZoneTwoDB.hudPoint = { p, "UIParent", rp, x, y }
    end)
    hud.text = hud:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
    hud.text:SetPoint("TOPLEFT", 10, -8)
    hud.text:SetJustifyH("LEFT")
end

local function refreshHud()
    if not hud then buildHud() end
    hud.text:SetText(hudText())
    hud:SetShown(ZoneTwoDB.hud)
end

-- Slash + events ----------------------------------------------------------
SLASH_ZONETWO1 = "/zt"
SLASH_ZONETWO2 = "/zonetwo"
SlashCmdList.ZONETWO = function(msg)
    msg = (msg or ""):lower():match("^%s*(%S*)")
    if msg == "on" or msg == "apply" then
        turnOn()
    elseif msg == "off" or msg == "restore" then
        turnOff()
    elseif msg == "autorun" then
        ZoneTwoDB.autorun = not ZoneTwoDB.autorun
        say("Up button = " .. (ZoneTwoDB.autorun and "autorun toggle" or "hold to move forward"))
        if ZoneTwoDB.enabled then turnOn(true) end
        refreshHud()
    elseif msg == "hud" then
        ZoneTwoDB.hud = not ZoneTwoDB.hud
        refreshHud()
    else
        say("/zt on | off | autorun | hud  (now " .. (ZoneTwoDB.enabled and "ON" or "OFF") .. ")")
    end
end

owner:RegisterEvent("ADDON_LOADED")
owner:RegisterEvent("PLAYER_LOGIN")
owner:SetScript("OnEvent", function(_, event, name)
    if event == "ADDON_LOADED" and name == ADDON then
        ZoneTwoDB = ZoneTwoDB or {}
        if ZoneTwoDB.hud == nil then ZoneTwoDB.hud = true end
        refreshHud()
    elseif event == "PLAYER_LOGIN" then
        migrateLegacySnapshot()
        if ZoneTwoDB.enabled then turnOn(true) end
    elseif event == "PLAYER_REGEN_ENABLED" then
        owner:UnregisterEvent("PLAYER_REGEN_ENABLED")
        local fn = pendingAfterCombat
        pendingAfterCombat = nil
        if fn then fn() end
    end
end)
