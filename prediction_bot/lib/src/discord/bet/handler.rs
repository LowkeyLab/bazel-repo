//! Discord adapters for the betting flow; mutations use the existing store transaction.
use super::{Action, panel, parse, preview, stake_modal};
use crate::discord::{
    ActionRowComponent, Actor, ComponentInteraction, CreateInteractionResponse, Duration, GuildId,
    Handler, Http, InteractionTransport, ModalInteraction, QueryKind, SerenityTransport, Stage,
    UserId, deferred_response, delivery_outcome, execute_request, interaction_error,
    interaction_event, read_query, rejected, reply,
};
use crate::types::{MarketId, OutcomeIndex};

impl Handler {
    pub(in crate::discord) async fn handle_bet_component(
        &self,
        http: &Http,
        component: &ComponentInteraction,
    ) {
        let actor = Actor {
            user_id: UserId(component.user.id.get()),
            bot: component.user.bot,
            moderator: false,
        };
        let guild = component
            .guild_id
            .map_or(GuildId(0), |id| GuildId(id.get()));
        let action = parse(
            guild,
            actor,
            &component.data.custom_id,
            &component.data.kind,
        );
        if let Ok(Action::Stake { id, outcome }) = &action {
            self.open_bet_stake(http, component, actor, guild, id, *outcome)
                .await;
            return;
        }
        let transport = if action.is_ok() {
            SerenityTransport::ComponentUpdate(component, http)
        } else {
            SerenityTransport::Component(component, http)
        };
        deferred_response(
            &transport,
            self.store.audit().as_ref(),
            component.guild_id.map(|id| GuildId(id.get())),
            component.id.get(),
            || async {
                let result = match action {
                    Ok(Action::Confirm {
                        command,
                        submission,
                    }) => {
                        // Distinct clicks share the stake submission key. Replay must reach
                        // the store before current eligibility checks, even after closure.
                        return execute_request(&self.store, guild, actor, &command, submission)
                            .await
                            .embeds(vec![])
                            .components(vec![]);
                    }
                    Ok(Action::Cancel) => {
                        return reply("Bet cancelled. No points were staked.")
                            .embeds(vec![])
                            .components(vec![]);
                    }
                    Ok(action) => match read_query(
                        self.store.audit().as_ref(),
                        guild,
                        component.id.get(),
                        QueryKind::Component,
                        self.store.view(guild),
                        None,
                    )
                    .await
                    {
                        Ok(view) => {
                            panel(&view, actor, guild, chrono::Utc::now().timestamp(), &action)
                        }
                        Err(message) => return reply(&message).embeds(vec![]).components(vec![]),
                    },
                    Err(message) => Err(message),
                };
                match result {
                    Ok(panel) => panel.edit(),
                    Err(message) => {
                        rejected(self.store.audit().as_ref(), Some(guild), component.id.get());
                        reply(message).embeds(vec![]).components(vec![])
                    }
                }
            },
        )
        .await;
    }

    async fn open_bet_stake(
        &self,
        http: &Http,
        component: &ComponentInteraction,
        actor: Actor,
        guild: GuildId,
        id: &MarketId,
        outcome: OutcomeIndex,
    ) {
        // Opening a modal must be the initial response. Bound the database read.
        let response = match read_query(
            self.store.audit().as_ref(),
            guild,
            component.id.get(),
            QueryKind::Component,
            self.store.view(guild),
            Some(Duration::from_secs(2)),
        )
        .await
        {
            Ok(view) => match stake_modal(
                &view,
                actor,
                guild,
                chrono::Utc::now().timestamp(),
                id,
                outcome,
            ) {
                Ok(modal) => CreateInteractionResponse::Modal(modal),
                Err(message) => {
                    rejected(self.store.audit().as_ref(), Some(guild), component.id.get());
                    interaction_error(message)
                }
            },
            Err(message) => interaction_error(&message),
        };
        let result = SerenityTransport::Component(component, http)
            .respond(response)
            .await;
        interaction_event(
            self.store.audit().as_ref(),
            Some(guild),
            component.id.get(),
            Stage::Deliver,
            delivery_outcome(&result),
        );
    }

    pub(in crate::discord) async fn handle_bet_modal(&self, http: &Http, modal: &ModalInteraction) {
        let actor = Actor {
            user_id: UserId(modal.user.id.get()),
            bot: modal.user.bot,
            moderator: false,
        };
        let guild = modal.guild_id.map_or(GuildId(0), |id| GuildId(id.get()));
        deferred_response(
            &SerenityTransport::Modal(modal, http),
            self.store.audit().as_ref(),
            modal.guild_id.map(|id| GuildId(id.get())),
            modal.id.get(),
            || async {
                let fields: Vec<_> = modal
                    .data
                    .components
                    .iter()
                    .flat_map(|row| &row.components)
                    .collect();
                let amount = match fields.as_slice() {
                    [ActionRowComponent::InputText(field)] if field.custom_id == "amount" => {
                        field.value.as_deref()
                    }
                    _ => None,
                };
                let Some(amount) = amount else {
                    rejected(self.store.audit().as_ref(), Some(guild), modal.id.get());
                    return reply("Enter a single stake amount.");
                };
                let result = match read_query(
                    self.store.audit().as_ref(),
                    guild,
                    modal.id.get(),
                    QueryKind::Component,
                    self.store.view(guild),
                    None,
                )
                .await
                {
                    Ok(view) => preview(
                        &view,
                        actor,
                        guild,
                        chrono::Utc::now().timestamp(),
                        &modal.data.custom_id,
                        amount,
                        modal.id.get(),
                    ),
                    Err(message) => return reply(&message),
                };
                match result {
                    Ok(panel) => panel.edit(),
                    Err(message) => {
                        rejected(self.store.audit().as_ref(), Some(guild), modal.id.get());
                        reply(message)
                    }
                }
            },
        )
        .await;
    }
}
