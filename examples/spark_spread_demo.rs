//! Spark Spread Hedging Demo
//!
//! Shows how to hedge a gas-fired power plant using spark spread strategy

use hedging_engine::hedging::{CostsBreakdown, SparkSpreadHedge, SparkSpreadPositions};
use hedging_engine::*;

fn main() -> Result<()> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║         SPARK SPREAD HEDGING DEMO                          ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Power plant parameters
    let capacity_mw: f64 = 400.0; // 400 MW CCGT plant
    let heat_rate: f64 = 2.0; // 50% efficiency
    let emission_factor: f64 = 0.202; // Natural gas emissions
    let target_spread: f64 = 50.0; // Target €50/MWh spread

    println!("Power Plant Configuration:");
    println!("  Capacity:         {} MW", capacity_mw);
    println!(
        "  Heat Rate:        {} ({}% efficiency)",
        heat_rate,
        100.0 / heat_rate
    );
    println!("  Emission Factor:  {} tons CO2/MWh gas", emission_factor);
    println!("  Target Spread:    €{}/MWh\n", target_spread);

    // Create a hedge strategy
    let hedge: SparkSpreadHedge =
        SparkSpreadHedge::new(capacity_mw, heat_rate, emission_factor, target_spread);

    // Create orderbooks for 3 commodities
    let power_ob: OrderBook = OrderBook::new(1); // Power
    let gas_ob: OrderBook = OrderBook::new(2); // Gas (TTF)
    let co2_ob: OrderBook = OrderBook::new(3); // CO2 (EUA)

    // Scenario 1: Good spread (should hedge)
    println!("\n{}", "═".repeat(60));
    println!("SCENARIO 1: Profitable Spread");
    println!("\n{}", "═".repeat(60));

    let power_price: f64 = 100.0;
    let gas_price: f64 = 40.0;
    let co2_price: f64 = 80.0;

    println!("Market Prices:");
    println!("  Power:  €{:.2}/MWh", power_price);
    println!("  Gas:    €{:.2}/MWh", gas_price);
    println!("  CO2:    €{:.2}/ton\n", co2_price);

    // Update orderbooks
    power_ob.update_bid(0, (power_price * 10000.0) as i64, 1000, 1000);
    gas_ob.update_ask(0, (gas_price * 10000.0) as i64, 2000, 1000);
    co2_ob.update_ask(0, (co2_price * 10000.0) as i64, 500, 1000);

    // Calculate spread
    let spread: f64 = hedge.calculate_spread(power_price, gas_price, co2_price);
    println!("Spark Spread Calculation:");
    println!("  Power Price:           €{:.2}/MWh", power_price);
    println!(
        "  Gas Cost:              €{:.2}/MWh (€{} / {})",
        gas_price / heat_rate,
        gas_price,
        heat_rate
    );
    println!(
        "  CO2 Cost:              €{:.2}/MWh (€{} × {})",
        co2_price * emission_factor,
        co2_price,
        emission_factor
    );
    println!("  ─────────────────────────────────────");
    println!("  Spark Spread:          €{:.2}/MWh", spread);
    println!("  Target Spread:         €{:.2}/MWh", target_spread);
    println!(
        "  Profit above target:   €{:.2}/MWh",
        spread - target_spread
    );

    if hedge.is_profitable(spread) {
        println!("  Status:                PROFITABLE - HEDGE!\n");
    } else {
        println!("  Status:                NOT PROFITABLE\n");
    }

    // Get costs breakdown
    let costs: CostsBreakdown = hedge.calculate_costs_breakdown(gas_price, co2_price);
    println!("Cost Breakdown per MWh Power:");
    println!(
        "  Gas:                   €{:.2}/MWh",
        costs.gas_cost_per_mwh
    );
    println!(
        "  CO2:                   €{:.2}/MWh",
        costs.co2_cost_per_mwh
    );
    println!(
        "  Total Generation Cost: €{:.2}/MWh\n",
        costs.total_cost_per_mwh
    );

    // Get hedge recommendations
    let hours_ahead: f64 = 24.0; // Hedge 24 hours ahead

    if let Some(recs) = hedge.get_recommendations(&power_ob, &gas_ob, &co2_ob, hours_ahead) {
        println!("HEDGE RECOMMENDATIONS (24 hours ahead):");
        println!("─────────────────────────────────────\n");

        println!("1. POWER:");
        println!("   Action:   SELL {:.0} MWh", recs.power.quantity);
        println!("   Price:    €{:.2}/MWh", recs.power.price);
        println!(
            "   Revenue:  €{:.0}\n",
            recs.power.quantity * recs.power.price
        );

        println!("2. GAS:");
        println!("   Action:   BUY {:.0} MWh", recs.gas.quantity);
        println!("   Price:    €{:.2}/MWh", recs.gas.price);
        println!("   Cost:     €{:.0}\n", recs.gas.quantity * recs.gas.price);

        println!("3. CO2:");
        println!("   Action:   BUY {:.1} tons", recs.co2.quantity);
        println!("   Price:    €{:.2}/ton", recs.co2.price);
        println!("   Cost:     €{:.0}\n", recs.co2.quantity * recs.co2.price);

        println!("PROFITABILITY:");
        println!(
            "  Total Revenue:        €{:.0}",
            recs.power.quantity * recs.power.price
        );
        println!(
            "  Total Costs:          €{:.0}",
            recs.gas.quantity * recs.gas.price + recs.co2.quantity * recs.co2.price
        );
        println!("  Expected Profit:      €{:.0}", recs.total_profit);
        println!("  Profit per MWh:       €{:.2}/MWh\n", recs.profit_per_mwh);

        // Execute hedge
        hedge.execute_hedge(recs.power.quantity, recs.gas.quantity, recs.co2.quantity);

        println!("Hedge executed!\n");
    }

    // Show positions
    let positions: SparkSpreadPositions = hedge.get_positions();
    println!("Current Hedge Positions:");
    println!(
        "  Power:    {:.0} MW ({})",
        positions.power_mw.abs(),
        if positions.power_mw < 0.0 {
            "SOLD"
        } else {
            "BOUGHT"
        }
    );
    println!(
        "  Gas:      {:.0} MWh ({})",
        positions.gas_mwh.abs(),
        if positions.gas_mwh > 0.0 {
            "BOUGHT"
        } else {
            "SOLD"
        }
    );
    println!(
        "  CO2:      {:.1} tons ({})\n",
        positions.co2_tons.abs(),
        if positions.co2_tons > 0.0 {
            "BOUGHT"
        } else {
            "SOLD"
        }
    );

    // Scenario 2: Bad spread (should not hedge)
    println!("\n{}", "═".repeat(60));
    println!("SCENARIO 2: Unprofitable Spread");
    println!("\n{}", "═".repeat(60));

    let power_price2: f64 = 60.0;
    let gas_price2: f64 = 50.0;
    let co2_price2: f64 = 90.0;

    println!("Market Prices:");
    println!("  Power:  €{:.2}/MWh", power_price2);
    println!("  Gas:    €{:.2}/MWh", gas_price2);
    println!("  CO2:    €{:.2}/ton\n", co2_price2);

    let spread2: f64 = hedge.calculate_spread(power_price2, gas_price2, co2_price2);
    println!("Spark Spread:          €{:.2}/MWh", spread2);
    println!("Target Spread:         €{:.2}/MWh", target_spread);

    if hedge.is_profitable(spread2) {
        println!("Status:                PROFITABLE\n");
    } else {
        println!("Status:                NOT PROFITABLE - DON'T HEDGE\n");
        println!(
            "Reason: Spread (€{:.2}) below target (€{:.2})",
            spread2, target_spread
        );
        println!("Loss if hedged: €{:.2}/MWh\n", target_spread - spread2);
    }

    // Calculate P&L on existing position
    println!("\n{}", "═".repeat(60));
    println!("P&L ANALYSIS");
    println!("\n{}", "═".repeat(60));

    println!("Hedged at prices:");
    println!("  Power:  €{:.2}/MWh", power_price);
    println!("  Gas:    €{:.2}/MWh", gas_price);
    println!("  CO2:    €{:.2}/ton\n", co2_price);

    // Calculate P&L at original prices (should be ~0)
    let pnl_original: f64 = hedge.calculate_pnl(power_price, gas_price, co2_price);
    println!("P&L at hedge prices:   €{:.0}", pnl_original);

    // Calculate P&L at new prices
    let pnl_new: f64 = hedge.calculate_pnl(power_price2, gas_price2, co2_price2);
    println!("P&L at new prices:     €{:.0}", pnl_new);
    println!("Price movement impact: €{:.0}\n", pnl_new - pnl_original);

    // Scenario 3: Exceptional spread (high urgency)
    println!("\n{}", "═".repeat(60));
    println!("SCENARIO 3: Exceptional Spread (HIGH URGENCY)");
    println!("\n{}", "═".repeat(60));

    let power_price3: f64 = 120.0;
    let gas_price3: f64 = 35.0;
    let co2_price3: f64 = 70.0;

    let spread3: f64 = hedge.calculate_spread(power_price3, gas_price3, co2_price3);
    println!("Spark Spread:          €{:.2}/MWh", spread3);
    println!("Average Spread:        €{:.2}/MWh", spread);
    println!("Premium vs Average:    €{:.2}/MWh", spread3 - spread);
    println!("\nEXCEPTIONAL SPREAD - URGENT HEDGE RECOMMENDED!\n");

    Ok(())
}
