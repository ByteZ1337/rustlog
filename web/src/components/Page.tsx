import React from "react";
import styled from "styled-components";
import {Filters} from "./Filters";
import {LogContainer} from "./LogContainer";
import {BackgroundAnimation} from "./BackgroundAnimation";

const PageContainer = styled.div`
    position: relative;
    z-index: 1;
`;

export function Page() {
    return (
        <>
            <BackgroundAnimation/>
            <PageContainer>
                <Filters/>
                <LogContainer/>
            </PageContainer>
        </>
    );
}