-- Základní resource – obsahuje výchozí herní obsah.
-- Načítán jako první (load_order = 0).

fx_version 'rts1'
name       'base'
load_order = 0

-- Skripty sdílené pro server i klient (definice, konstanty, utility)
shared_scripts { 'shared/*.lua' }

-- Skripty jen pro server (herní logika, AI, pravidla)
server_scripts { 'server/*.lua' }

-- Skripty jen pro klienta (HUD, vizuální efekty, lokální UI)
client_scripts { 'client/*.lua' }
