use super::*;

impl RealtimeMatchService {
    pub async fn create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchCreate = msg.payload()?;
        let id = self.create_match(
            &session_of(conn),
            req.open,
            req.tick_rate,
            req.name.unwrap_or(req.label),
        );
        Ok(Some(EnvelopeMessage::MatchCreateAck {
            match_id: id.simple().to_string(),
        }))
    }
    pub async fn join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchJoin = msg.payload()?;
        self.join_match(
            &session_of(conn),
            req.match_id,
            req.token,
            req.label,
            msg.cid().map(str::to_string),
        )
        .map(|_| None)
    }
    pub async fn leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchLeave = msg.payload()?;
        self.leave_match(
            &session_of(conn),
            &req.match_id,
            msg.cid().map(str::to_string),
        )
        .map(|_| None)
    }
    pub async fn data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchDataSend = msg.payload()?;
        self.data_send_match(&session_of(conn), &req.match_id, req.op_code, req.data)
            .map(|_| None)
    }
    pub async fn signal(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchSignal = msg.payload()?;
        self.signal_match(
            &session_of(conn),
            &req.match_id,
            req.data,
            msg.cid().map(str::to_string),
        )
        .map(|_| None)
    }
}
