import { useContext } from "react";
import { useQuery } from "react-query";
import { store } from "../store";

export interface Channel {
    userID: string,
    name: string
}

export function useChannels(): Array<Channel> {
    const { state } = useContext(store);

    const { data } = useQuery<Array<Channel>>(`channels`, async () => {
        const queryUrl = new URL(`${state.apiBaseUrl}/channels`);

        return fetch(queryUrl.toString())
            .then(response => response.ok ? response.json() : { channels: [] })
            .then((data: { channels: Array<Channel> }) => data.channels);
    }, { refetchOnWindowFocus: false, refetchOnReconnect: false });

    return data ?? [];
}