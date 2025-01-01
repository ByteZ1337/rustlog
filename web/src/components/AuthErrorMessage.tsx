import styled from "styled-components";
import React from "react";

const AuthErrorContainer = styled.div`
    display: block;
    font-weight: bold;
    color: var(--danger);
    font-size: 2rem;
    text-align: center;
    padding: 2rem;
`;

export function AuthErrorMessage() {
    return <AuthErrorContainer>Invalid key</AuthErrorContainer>
}