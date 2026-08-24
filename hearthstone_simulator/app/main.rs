use hearthstone_simulator_core::{Card, GameAction, PlayerConfig, PlayerId, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Player One", vec![Card::minion("Training Minion", 1, 1, 2)]),
        PlayerConfig::new("Player Two", Vec::new()),
    ]);

    simulation.apply(GameAction::PlayCard {
        player: PlayerId::One,
        hand_index: 0,
    })?;
    simulation.apply(GameAction::EndTurn {
        player: PlayerId::One,
    })?;

    println!("{:#?}", simulation.snapshot());
    Ok(())
}
