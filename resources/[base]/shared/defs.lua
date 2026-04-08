-- Sdílené definice – načítají se na serveru i klientu.

TILE_SIZE = 32

-- Barvy týmů
TEAM_COLORS = {
    [0] = {0.20, 0.50, 1.00},  -- modrá  (hráč)
    [1] = {0.90, 0.20, 0.20},  -- červená (nepřítel)
    [2] = {0.20, 0.80, 0.20},  -- zelená
    [3] = {0.90, 0.80, 0.10},  -- žlutá
}

-- Náklady na výcvik (gold, lumber) – informativní pro UI
UNIT_COSTS = {
    peasant          = { gold=400,  lumber=0   },
    peon             = { gold=400,  lumber=0   },
    footman          = { gold=600,  lumber=0   },
    archer           = { gold=500,  lumber=50  },
    knight           = { gold=800,  lumber=100 },
    mage             = { gold=1200, lumber=200 },
    grunt            = { gold=600,  lumber=0   },
    troll_axethrower = { gold=500,  lumber=50  },
    ogre             = { gold=800,  lumber=100 },
    death_knight     = { gold=1200, lumber=200 },
}

-- Čas výcviku (sekundy)
UNIT_TRAIN_TIME = {
    peasant          = 15,
    peon             = 15,
    footman          = 20,
    archer           = 20,
    knight           = 30,
    mage             = 35,
    grunt            = 20,
    troll_axethrower = 20,
    ogre             = 30,
    death_knight     = 35,
}
