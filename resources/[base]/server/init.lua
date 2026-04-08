-- Serverový init – výchozí herní logika.

-- Příklad: pošli HUD update klientovi při změně zdrojů
AddEventHandler('player:requestResources', function(source)
    -- source je player_id klienta, který zavolal TriggerServerEvent
    Engine.log('Hráč ' .. tostring(source) .. ' požaduje stav zdrojů')
    -- TriggerClientEvent('hud:resourceUpdate', source, gold, lumber, oil)
end)

-- AI definice (přesunout sem z globálních skriptů)
AiDefs = AiDefs or {}
