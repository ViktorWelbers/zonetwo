-- ZoneTwo: play WoW with Zwift Ride controllers (the zonetwo program sends the keys).
-- /zt apply    bind keys + enable soft targeting (snapshot taken first)
-- /zt restore  put original bindings + CVars back
-- /zt autorun  toggle fallback: Up = autorun toggle (if BikeControl can't hold keys)
-- /zt hud      show/hide the button cheat-sheet

local ADDON = ...
local PREFIX = "|cff33ff99ZoneTwo|r: "

-- Ride button -> key BikeControl sends -> WoW binding command
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
    { button = "L shift -",   key = "F",      cmd = "INTERACTTARGET",     label = "Interact" },
    { button = "R paddle",    key = "SPACE",  cmd = "JUMP",               label = "Jump" },
    { button = "L paddle",    key = "ESCAPE", cmd = nil,                  label = "Clear / close" }, -- default Esc behaviour
    { button = "R power-up",  key = "7",      cmd = "ACTIONBUTTON7",      label = "Bar 7 (mount)" },
    { button = "L power-up",  key = "8",      cmd = "ACTIONBUTTON8",      label = "Bar 8 (heal/pot)" },
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

local function saveBindings()
    local set = GetCurrentBindingSet and GetCurrentBindingSet() or 1
    SaveBindings(set)
end

local function blockedByCombat()
    if InCombatLockdown() then
        say("can't change bindings in combat. Try again after combat.")
        return true
    end
end

local function upCommand()
    return ZoneTwoDB.autorun and "TOGGLEAUTORUN" or "MOVEFORWARD"
end

local function apply()
    if blockedByCombat() then return end

    if not ZoneTwoDB.snapshot then
        local snap = { bindings = {}, cvars = {} }
        for _, m in ipairs(MAP) do
            if m.cmd then snap.bindings[m.key] = GetBindingAction(m.key) or "" end
        end
        for name in pairs(CVARS) do snap.cvars[name] = getCVar(name) end
        ZoneTwoDB.snapshot = snap
    end

    local bound = 0
    for _, m in ipairs(MAP) do
        local cmd = m.cmd
        if m.button == "Up" then cmd = upCommand() end
        if cmd then
            if SetBinding(m.key, cmd) then bound = bound + 1
            else say("could not bind " .. m.key .. " -> " .. cmd) end
        end
    end
    saveBindings()

    local changed, missing = {}, {}
    for name, value in pairs(CVARS) do
        if getCVar(name) ~= nil then
            setCVar(name, value)
            changed[#changed + 1] = name
        else
            missing[#missing + 1] = name
        end
    end

    ZoneTwoDB.applied = true
    say(("bound %d keys; set %s"):format(bound, table.concat(changed, ", ")))
    if #missing > 0 then say("not in this client (skipped): " .. table.concat(missing, ", ")) end
end

local function restore()
    if blockedByCombat() then return end
    local snap = ZoneTwoDB.snapshot
    if not snap then say("nothing to restore.") return end

    for key, action in pairs(snap.bindings) do
        if action == "" then SetBinding(key) else SetBinding(key, action) end
    end
    saveBindings()
    for name, value in pairs(snap.cvars) do
        if value ~= nil then setCVar(name, value) end
    end

    ZoneTwoDB.snapshot = nil
    ZoneTwoDB.applied = false
    say("original bindings and settings restored.")
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
    if msg == "apply" then
        apply()
    elseif msg == "restore" then
        restore()
    elseif msg == "autorun" then
        ZoneTwoDB.autorun = not ZoneTwoDB.autorun
        say("Up button = " .. (ZoneTwoDB.autorun and "autorun toggle" or "hold to move forward"))
        if ZoneTwoDB.applied then apply() end
        refreshHud()
    elseif msg == "hud" then
        ZoneTwoDB.hud = not ZoneTwoDB.hud
        refreshHud()
    else
        say("/zt apply | restore | autorun | hud")
    end
end

local f = CreateFrame("Frame")
f:RegisterEvent("ADDON_LOADED")
f:SetScript("OnEvent", function(_, _, name)
    if name ~= ADDON then return end
    ZoneTwoDB = ZoneTwoDB or {}
    if ZoneTwoDB.hud == nil then ZoneTwoDB.hud = true end
    refreshHud()
    f:UnregisterEvent("ADDON_LOADED")
end)
