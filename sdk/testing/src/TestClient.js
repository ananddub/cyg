/** One headless player. Inputs are delivered by TestRoom in seq order. */
export class TestClient {
    constructor(room, id, options = {}) {
        this.room = room;
        this.id = id;
        this.options = options;
        this.connected = true;
        this.state = null;
        this._seq = 0;
        this._listeners = new Map();
    }

    sendInput(input) {
        if (!this.connected) return false;
        this._seq = (this._seq + 1) & 0xffff;
        return this.room._queueInput(this, this._seq, input);
    }

    send(type, payload) {
        if (!this.connected) return false;
        this.room._queueMessage(this, type, payload);
        return true;
    }

    on(type, handler) {
        const list = this._listeners.get(type) || [];
        list.push(handler);
        this._listeners.set(type, list);
        return () => {
            const index = list.indexOf(handler);
            if (index >= 0) list.splice(index, 1);
        };
    }

    emit(type, payload) {
        for (const handler of this._listeners.get(type) || []) handler(payload);
    }

    leave() {
        if (!this.connected) return;
        this.connected = false;
        this.room._leave(this);
    }
}
