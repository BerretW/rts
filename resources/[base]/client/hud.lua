-- Klientský HUD skript – reaguje na eventy ze serveru.

AddEventHandler('hud:resourceUpdate', function(gold, lumber, oil)
    -- Klientský skript může v budoucnu modifikovat zobrazení zdrojů
    Engine.log('Zdroje: zlato=' .. tostring(gold) .. ' dřevo=' .. tostring(lumber))
end)

AddEventHandler('hud:notification', function(text)
    Engine.log('[HUD] ' .. tostring(text))
end)
