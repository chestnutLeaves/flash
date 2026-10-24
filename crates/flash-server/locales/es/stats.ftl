### The open core's words in Spanish, both variants: stats.

## Estadísticas

stats-title = Estadísticas - Flash
stats-heading = Estadísticas
stats-reviews-30 = repasos, últimos 30 días
stats-retention = retención, últimos 30 días
stats-reviews-all = repasos en total
stats-streak = días de racha
stats-longest = racha más larga
stats-mature = tarjetas maduras (de las activas)
stats-percent = { $n }%
stats-past-year = Último año
stats-years = Año del mapa de calor
stats-cell-due =
    { $count ->
        [one] { $day }: { $count } pendiente
        [many] { $day }: { $count } pendientes
       *[other] { $day }: { $count } pendientes
    }
stats-cell-reviews =
    { $count ->
        [one] { $day }: { $count } repaso
        [many] { $day }: { $count } de repasos
       *[other] { $day }: { $count } repasos
    }
stats-legend-avg-html =
    { $count ->
        [one] Promedio diario: <span>{ $count } repaso</span>
        [many] Promedio diario: <span>{ $count } de repasos</span>
       *[other] Promedio diario: <span>{ $count } repasos</span>
    }
stats-legend-learned-html = Días de estudio: <span>{ $pct }%</span>
stats-legend-longest-html =
    { $count ->
        [one] Racha más larga: <span>{ $count } día</span>
        [many] Racha más larga: <span>{ $count } de días</span>
       *[other] Racha más larga: <span>{ $count } días</span>
    }
stats-legend-current-html =
    { $count ->
        [one] Racha actual: <span>{ $count } día</span>
        [many] Racha actual: <span>{ $count } de días</span>
       *[other] Racha actual: <span>{ $count } días</span>
    }
stats-per-day = Repasos por día
stats-coming-up = Próximos días
stats-col-day = Día
stats-col-due = Pendientes
stats-today = Hoy

## Las cuentas del servidor (solo para quienes administran)

users-title = Usuarios - Flash
users-heading = Usuarios
users-col-user = Usuario
users-col-email = Correo
users-col-joined = Registro
users-col-decks = Mazos
users-col-cards = Tarjetas
users-col-reviews = Repasos
users-col-studied = Último estudio
users-col-login = Último inicio de sesión
users-col-mcp = MCP
users-admin = Admin
