//! HTML dashboard for usage visualization

pub const DASHBOARD_HTML: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Tark Usage Dashboard</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: #0f172a;
            color: #e2e8f0;
            padding: 20px;
        }
        
        .container {
            max-width: 1400px;
            margin: 0 auto;
        }
        
        header {
            margin-bottom: 30px;
            padding-bottom: 20px;
            border-bottom: 2px solid #1e293b;
        }
        
        h1 {
            font-size: 2.5rem;
            background: linear-gradient(135deg, #3b82f6 0%, #8b5cf6 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 10px;
        }
        
        .subtitle {
            color: #94a3b8;
            font-size: 1.1rem;
        }
        
        .stats-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }
        
        .stat-card {
            background: #1e293b;
            border-radius: 12px;
            padding: 24px;
            border: 1px solid #334155;
            transition: transform 0.2s, box-shadow 0.2s;
        }
        
        .stat-card:hover {
            transform: translateY(-2px);
            box-shadow: 0 8px 24px rgba(59, 130, 246, 0.2);
        }
        
        .stat-label {
            font-size: 0.875rem;
            color: #94a3b8;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            margin-bottom: 8px;
        }
        
        .stat-value {
            font-size: 2rem;
            font-weight: 700;
            color: #e2e8f0;
        }
        
        .stat-change {
            font-size: 0.875rem;
            margin-top: 8px;
        }
        
        .stat-change.positive {
            color: #10b981;
        }
        
        .chart-container {
            background: #1e293b;
            border-radius: 12px;
            padding: 24px;
            margin-bottom: 30px;
            border: 1px solid #334155;
        }
        
        .chart-title {
            font-size: 1.25rem;
            font-weight: 600;
            margin-bottom: 20px;
            color: #e2e8f0;
        }
        
        .chart-wrapper {
            position: relative;
            height: 300px;
        }
        
        .chart-wrapper.large {
            height: 400px;
        }
        
        .controls {
            display: flex;
            gap: 12px;
            margin-bottom: 20px;
            flex-wrap: wrap;
        }
        
        button {
            background: #3b82f6;
            color: white;
            border: none;
            padding: 10px 20px;
            border-radius: 8px;
            cursor: pointer;
            font-size: 0.875rem;
            font-weight: 600;
            transition: background 0.2s;
        }
        
        button:hover {
            background: #2563eb;
        }
        
        button.secondary {
            background: #475569;
        }
        
        button.secondary:hover {
            background: #64748b;
        }
        
        button.danger {
            background: #ef4444;
        }
        
        button.danger:hover {
            background: #dc2626;
        }
        
        .table-container {
            overflow-x: auto;
        }
        
        table {
            width: 100%;
            border-collapse: collapse;
        }
        
        th, td {
            text-align: left;
            padding: 12px;
            border-bottom: 1px solid #334155;
        }
        
        th {
            background: #0f172a;
            color: #94a3b8;
            font-weight: 600;
            font-size: 0.875rem;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        
        tr:hover {
            background: #334155;
        }
        
        .badge {
            display: inline-block;
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: 600;
        }
        
        .badge.chat {
            background: #3b82f6;
        }
        
        .badge.fim {
            background: #8b5cf6;
        }
        
        .loading {
            text-align: center;
            padding: 40px;
            color: #94a3b8;
        }
        
        .error {
            background: #ef4444;
            color: white;
            padding: 16px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        
        .refresh-time {
            color: #64748b;
            font-size: 0.875rem;
            margin-top: 10px;
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>Tark Usage Dashboard</h1>
            <p class="subtitle">Token usage, costs, and analytics</p>
            <div class="refresh-time">Last updated: <span id="refresh-time">--</span></div>
        </header>
        
        <div class="controls">
            <button onclick="refreshData()">Refresh Data</button>
            <button class="secondary" onclick="exportData()">Export CSV</button>
            <button class="danger" onclick="cleanupLogs()">Cleanup Old Logs</button>
        </div>
        
        <div id="error-container"></div>
        
        <div class="stats-grid">
            <div class="stat-card">
                <div class="stat-label">Total Cost</div>
                <div class="stat-value" id="total-cost">$0.00</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Total Tokens</div>
                <div class="stat-value" id="total-tokens">0</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Sessions</div>
                <div class="stat-value" id="session-count">0</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Requests</div>
                <div class="stat-value" id="request-count">0</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Database Size</div>
                <div class="stat-value" id="db-size">0 KB</div>
            </div>
        </div>
        
        <div class="chart-container">
            <h2 class="chart-title">Cost by Model</h2>
            <div class="chart-wrapper">
                <canvas id="model-cost-chart"></canvas>
            </div>
        </div>
        
        <div class="chart-container">
            <h2 class="chart-title">Token Usage by Model</h2>
            <div class="chart-wrapper">
                <canvas id="model-tokens-chart"></canvas>
            </div>
        </div>
        
        <div class="chart-container">
            <h2 class="chart-title">Usage by Mode</h2>
            <div class="chart-wrapper">
                <canvas id="mode-chart"></canvas>
            </div>
        </div>
        
        <div class="chart-container">
            <h2 class="chart-title">Model Usage Details</h2>
            <div class="table-container">
                <table id="model-table">
                    <thead>
                        <tr>
                            <th>Provider</th>
                            <th>Model</th>
                            <th>Requests</th>
                            <th>Input Tokens</th>
                            <th>Output Tokens</th>
                            <th>Cost</th>
                        </tr>
                    </thead>
                    <tbody id="model-table-body">
                        <tr><td colspan="6" class="loading">Loading...</td></tr>
                    </tbody>
                </table>
            </div>
        </div>
        
        <div class="chart-container">
            <h2 class="chart-title">Recent Sessions</h2>
            <div class="table-container">
                <table id="sessions-table">
                    <thead>
                        <tr>
                            <th>Host</th>
                            <th>Username</th>
                            <th>Session Name</th>
                            <th>Logs</th>
                            <th>Tokens</th>
                            <th>Cost</th>
                            <th>Created</th>
                        </tr>
                    </thead>
                    <tbody id="sessions-table-body">
                        <tr><td colspan="7" class="loading">Loading...</td></tr>
                    </tbody>
                </table>
            </div>
        </div>
    </div>
    
    <script>
        let modelCostChart, modelTokensChart, modeChart;
        
        function formatCost(cost) {
            if (cost === undefined || cost === null || isNaN(cost)) return '$0.00';
            if (cost < 0.01) return `$${cost.toFixed(4)}`;
            if (cost < 1) return `$${cost.toFixed(3)}`;
            return `$${cost.toFixed(2)}`;
        }
        
        function formatNumber(n) {
            if (n === undefined || n === null || isNaN(n)) return '0';
            if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
            if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
            return n.toString();
        }
        
        function showError(message) {
            const container = document.getElementById('error-container');
            container.innerHTML = `<div class="error">${message}</div>`;
            setTimeout(() => container.innerHTML = '', 5000);
        }
        
        async function refreshData() {
            try {
                // Fetch summary
                const summaryRes = await fetch('/api/usage/summary');
                const summary = await summaryRes.json();
                
                // Update summary stats with safe defaults
                document.getElementById('total-cost').textContent = formatCost(summary.total_cost);
                document.getElementById('total-tokens').textContent = formatNumber(summary.total_tokens);
                document.getElementById('session-count').textContent = summary.session_count || 0;
                document.getElementById('request-count').textContent = summary.log_count || 0;
                document.getElementById('db-size').textContent = summary.db_size_human || '0 B';
                
                // Fetch model usage
                const modelsRes = await fetch('/api/usage/models');
                const models = await modelsRes.json();
                updateModelCharts(models);
                updateModelTable(models);
                
                // Fetch mode usage
                const modesRes = await fetch('/api/usage/modes');
                const modes = await modesRes.json();
                updateModeChart(modes);
                
                // Fetch sessions
                const sessionsRes = await fetch('/api/usage/sessions');
                const sessions = await sessionsRes.json();
                updateSessionsTable(sessions);
                
                // Update refresh time
                document.getElementById('refresh-time').textContent = new Date().toLocaleTimeString();
            } catch (err) {
                showError(`Failed to refresh data: ${err.message}`);
            }
        }
        
        function updateModelCharts(models) {
            const labels = models.map(m => `${m.provider}/${m.model}`);
            const costs = models.map(m => m.cost);
            const inputTokens = models.map(m => m.input_tokens);
            const outputTokens = models.map(m => m.output_tokens);
            
            // Cost chart
            if (modelCostChart) modelCostChart.destroy();
            const costCtx = document.getElementById('model-cost-chart').getContext('2d');
            modelCostChart = new Chart(costCtx, {
                type: 'bar',
                data: {
                    labels,
                    datasets: [{
                        label: 'Cost ($)',
                        data: costs,
                        backgroundColor: 'rgba(59, 130, 246, 0.8)',
                        borderColor: 'rgba(59, 130, 246, 1)',
                        borderWidth: 1
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        legend: { display: false }
                    },
                    scales: {
                        y: {
                            beginAtZero: true,
                            ticks: { color: '#94a3b8' },
                            grid: { color: '#334155' }
                        },
                        x: {
                            ticks: { color: '#94a3b8' },
                            grid: { color: '#334155' }
                        }
                    }
                }
            });
            
            // Tokens chart
            if (modelTokensChart) modelTokensChart.destroy();
            const tokensCtx = document.getElementById('model-tokens-chart').getContext('2d');
            modelTokensChart = new Chart(tokensCtx, {
                type: 'bar',
                data: {
                    labels,
                    datasets: [
                        {
                            label: 'Input Tokens',
                            data: inputTokens,
                            backgroundColor: 'rgba(139, 92, 246, 0.8)',
                            borderColor: 'rgba(139, 92, 246, 1)',
                            borderWidth: 1
                        },
                        {
                            label: 'Output Tokens',
                            data: outputTokens,
                            backgroundColor: 'rgba(59, 130, 246, 0.8)',
                            borderColor: 'rgba(59, 130, 246, 1)',
                            borderWidth: 1
                        }
                    ]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        legend: { 
                            labels: { color: '#94a3b8' }
                        }
                    },
                    scales: {
                        y: {
                            stacked: true,
                            beginAtZero: true,
                            ticks: { color: '#94a3b8' },
                            grid: { color: '#334155' }
                        },
                        x: {
                            stacked: true,
                            ticks: { color: '#94a3b8' },
                            grid: { color: '#334155' }
                        }
                    }
                }
            });
        }
        
        function updateModeChart(modes) {
            const labels = modes.map(m => `${m.request_type} (${m.mode})`);
            const costs = modes.map(m => m.cost);
            
            if (modeChart) modeChart.destroy();
            const ctx = document.getElementById('mode-chart').getContext('2d');
            modeChart = new Chart(ctx, {
                type: 'doughnut',
                data: {
                    labels,
                    datasets: [{
                        data: costs,
                        backgroundColor: [
                            'rgba(59, 130, 246, 0.8)',
                            'rgba(139, 92, 246, 0.8)',
                            'rgba(236, 72, 153, 0.8)',
                            'rgba(34, 197, 94, 0.8)',
                            'rgba(251, 146, 60, 0.8)'
                        ],
                        borderColor: '#1e293b',
                        borderWidth: 2
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        legend: {
                            labels: { color: '#94a3b8' },
                            position: 'right'
                        }
                    }
                }
            });
        }
        
        function updateModelTable(models) {
            const tbody = document.getElementById('model-table-body');
            tbody.innerHTML = models.map(m => `
                <tr>
                    <td>${m.provider}</td>
                    <td>${m.model}</td>
                    <td>${m.request_count}</td>
                    <td>${formatNumber(m.input_tokens)}</td>
                    <td>${formatNumber(m.output_tokens)}</td>
                    <td>${formatCost(m.cost)}</td>
                </tr>
            `).join('');
        }
        
        function updateSessionsTable(sessions) {
            const tbody = document.getElementById('sessions-table-body');
            tbody.innerHTML = sessions.slice(0, 20).map(s => `
                <tr>
                    <td>${s.host}</td>
                    <td>${s.username}</td>
                    <td>${s.name || '(unnamed)'}</td>
                    <td>${s.log_count}</td>
                    <td>${formatNumber(s.total_tokens)}</td>
                    <td>${formatCost(s.total_cost)}</td>
                    <td>${new Date(s.created_at).toLocaleDateString()}</td>
                </tr>
            `).join('');
        }
        
        async function exportData() {
            try {
                const response = await fetch('/api/usage/export');
                const blob = await response.blob();
                const url = window.URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = `tark-usage-${new Date().toISOString().split('T')[0]}.csv`;
                document.body.appendChild(a);
                a.click();
                document.body.removeChild(a);
                window.URL.revokeObjectURL(url);
            } catch (err) {
                showError(`Export failed: ${err.message}`);
            }
        }
        
        function cleanupLogs() {
            const days = prompt('Delete logs older than how many days?', '30');
            if (!days) return;
            
            if (confirm(`Are you sure you want to delete logs older than ${days} days?`)) {
                fetch('/api/usage/cleanup', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ older_than_days: parseInt(days) })
                })
                .then(res => res.json())
                .then(data => {
                    alert(`Cleaned up ${data.deleted_logs} logs, freed ${data.freed_human}`);
                    refreshData();
                })
                .catch(err => showError(`Cleanup failed: ${err.message}`));
            }
        }
        
        // Initial load
        refreshData();
        
        // Auto-refresh every 30 seconds
        setInterval(refreshData, 30000);
    </script>
</body>
</html>
"#;
